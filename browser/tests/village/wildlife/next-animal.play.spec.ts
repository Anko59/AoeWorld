import { readFile } from "node:fs/promises";
import { expect, type Page } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { centerVillage, freshHud } from "../fixtures/resources.js";

type Actor = {
  id: number;
  kind: string;
  x: number;
  y: number;
  hp: number;
  task: string;
};
const actors = async (page: Page): Promise<Actor[]> =>
  JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as Actor[];

test("shepherds exhaust one sheep and start another without a second hunt order", async ({
  page,
  request,
}) => {
  test.setTimeout(240_000);
  const capture = await readFile("../.cache/qol/capture.json", "utf8")
    .then((text) => JSON.parse(text) as { url: string })
    .catch(() => null);
  if (capture && capture.url !== "http://127.0.0.1:8131/")
    throw new Error("Unexpected quality-of-life capture origin");
  const base = capture?.url ?? "/";
  expect((await request.post(`${base}maps/reset`)).status()).toBe(204);
  await gameAssets(page);
  await page.goto(`${base}?play=1#zoom=.6`);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await centerVillage(page);
  const all = await actors(page);
  const first = all.find((a) => a.kind === "Sheep" && a.hp > 0);
  const scout = all.find((a) => a.kind === "Scout");
  if (!first || !scout) throw new Error("Starting sheep/scout missing");
  // Claim the nearby herd through a normal move, before any hunt is issued.
  await page.mouse.click(scout.x, scout.y);
  await page.locator("#move-units").click();
  await page.mouse.click(first.x, first.y, { button: "right" });
  // Poll the HUD in the browser so every observation does not add report steps.
  // Keep the original distance, HP predicates and deadlines for all three waits.
  await test.step(
    "Scout claims the nearby herd",
    async () => {
      await page.waitForFunction(
        ({ scoutId, first }) => {
          const all = JSON.parse(
            document.querySelector("#scene")?.getAttribute("data-actors") ??
              "[]",
          ) as Actor[];
          const current = all.find((a) => a.id === scoutId);
          return (
            !!current &&
            Math.hypot(current.x - first.x, current.y - first.y) < 5
          );
        },
        { scoutId: scout.id, first },
        { timeout: 15_000, polling: 250 },
      );
    },
    { box: true },
  );
  await freshHud(page);
  const villager = (await actors(page))
    .filter((a) => a.kind === "Villager")
    .sort((a, b) => b.y - a.y)[0];
  if (!villager) throw new Error("Starting villager missing");
  await page.mouse.dblclick(villager.x, villager.y);
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "3");
  const sheep = (await actors(page)).find((a) => a.id === first.id);
  if (!sheep) throw new Error("First sheep disappeared before the order");
  // This is the only hunt input. A second animal losing HP proves continuation.
  await page.mouse.click(sheep.x, sheep.y, { button: "right" });
  await test.step(
    "The first sheep is killed",
    async () => {
      await page.waitForFunction(
        (firstId) => {
          const all = JSON.parse(
            document.querySelector("#scene")?.getAttribute("data-actors") ??
              "[]",
          ) as Actor[];
          return all.find((a) => a.id === firstId)?.hp === 0;
        },
        first.id,
        { timeout: 25_000, polling: 250 },
      );
    },
    { box: true },
  );
  await test.step(
    "Shepherds start another sheep without another order",
    async () => {
      await page.waitForFunction(
        (firstId) => {
          const all = JSON.parse(
            document.querySelector("#scene")?.getAttribute("data-actors") ??
              "[]",
          ) as Actor[];
          return all.some(
            (a) => a.kind === "Sheep" && a.id !== firstId && a.hp < 7,
          );
        },
        first.id,
        { timeout: 180_000, polling: 1_000 },
      );
    },
    { box: true },
  );
  await freshHud(page);
  if (capture) {
    await page.screenshot({
      path: `../.cache/qol/next-animal-${test.info().project.name}.png`,
    });
  }
});
