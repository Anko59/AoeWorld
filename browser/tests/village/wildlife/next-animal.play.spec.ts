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
  let all = await actors(page);
  const first = all.find((a) => a.kind === "Sheep" && a.hp > 0);
  const scout = all.find((a) => a.kind === "Scout");
  if (!first || !scout) throw new Error("Starting sheep/scout missing");
  // Claim the nearby herd through a normal move, before any hunt is issued.
  await page.mouse.click(scout.x, scout.y);
  await page.locator("#move-units").click();
  await page.mouse.click(first.x, first.y, { button: "right" });
  await expect
    .poll(
      async () => {
        all = await actors(page);
        const current = all.find((a) => a.id === scout.id);
        return current
          ? Math.hypot(current.x - first.x, current.y - first.y)
          : Infinity;
      },
      { timeout: 15_000 },
    )
    .toBeLessThan(5);
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
  await expect
    .poll(async () => (await actors(page)).find((a) => a.id === first.id)?.hp, {
      timeout: 25_000,
    })
    .toBe(0);
  await expect
    .poll(
      async () =>
        (await actors(page)).some(
          (a) => a.kind === "Sheep" && a.id !== first.id && a.hp < 7,
        ),
      { timeout: 180_000 },
    )
    .toBe(true);
  await freshHud(page);
  if (capture) {
    await page.screenshot({
      path: `../.cache/qol/next-animal-${test.info().project.name}.png`,
    });
  }
});
