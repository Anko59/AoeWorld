import { expect, type Page } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { centerVillage, freshHud } from "./fixtures/resources.js";
import { readFile } from "node:fs/promises";
import { captureUnits } from "./units/capture.js";

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

test("an owned sheep stops herding when Stop is pressed", async ({
  page,
  request,
}) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
  await page.goto("/?play=1#zoom=.45");
  await expect(page.locator("#connection")).toHaveText("connected");
  await centerVillage(page);
  await freshHud(page);
  const neutral = (await actors(page)).find(
    (a) => a.kind === "Sheep" && a.hp > 0,
  );
  if (!neutral) throw new Error("Sheep missing from the authoritative HUD");
  const scout = await click(page, "Scout");
  await page.locator("#move-units").click();
  await page.mouse.click(neutral.x, neutral.y, { button: "right" });
  await expect
    .poll(async () => {
      const current = (await actors(page)).find((a) => a.id === scout.id);
      return current
        ? Math.hypot(current.x - neutral.x, current.y - neutral.y)
        : Infinity;
    })
    .toBeLessThan(3);
  const sheep = await click(page, "Sheep");
  await expect(page.locator("#selection-name")).toHaveText("Sheep");
  await expect(page.locator("#stop-units")).toBeVisible();
  const destination = { x: sheep.x + 180, y: sheep.y - 40 };
  await page.mouse.click(destination.x, destination.y, { button: "right" });
  await expect
    .poll(async () => {
      const current = (await actors(page)).find((a) => a.id === sheep.id);
      return current ? Math.hypot(current.x - sheep.x, current.y - sheep.y) : 0;
    })
    .toBeGreaterThan(3);
  await page.locator("#stop-units").click();
  await freshHud(page);
  await freshHud(page);
  const stopped = (await actors(page)).find((a) => a.id === sheep.id);
  if (!stopped) throw new Error("Selected sheep missing after Stop");
  for (let refresh = 0; refresh < 6; refresh++) await freshHud(page);
  const later = (await actors(page)).find((a) => a.id === sheep.id);
  expect(later && [later.x, later.y]).toEqual([stopped.x, stopped.y]);
  expect(
    Math.hypot(stopped.x - destination.x, stopped.y - destination.y),
  ).toBeGreaterThan(20);
});

test("private native gameplay captures and prepares the units showcase", async ({
  page,
}) => {
  const config = await readFile("../.cache/units-capture.json", "utf8")
    .then((text) => JSON.parse(text) as { url: string })
    .catch(() => null);
  test.skip(!config, "Native gameplay capture was not requested");
  if (!config || config.url !== "http://127.0.0.1:18084/")
    throw new Error("Unexpected units capture origin");
  // Two complete village preparations surround the combat and corpse captures.
  test.setTimeout(600_000);
  await captureUnits(page, config.url);
});
async function click(
  page: Page,
  kind: string,
  button: "left" | "right" = "left",
) {
  await freshHud(page);
  const actor = (await actors(page)).find((a) => a.kind === kind && a.hp > 0);
  if (!actor) throw new Error(`${kind} missing from the authoritative HUD`);
  await page.mouse.click(actor.x, actor.y, { button });
  return actor;
}

test("train Militia, attack a sheep, and hunt and butcher a deer through right-click orders", async ({
  page,
  request,
}) => {
  test.setTimeout(210_000);
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
  await page.goto("/?play=1#zoom=.45");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await centerVillage(page);
  await click(page, "Villager");
  await page.locator("#build-menu").click();
  await page.locator("#build-barracks").click();
  let placed = false;
  for (const [x, y] of [
    [400, 290],
    [850, 300],
    [420, 430],
    [900, 420],
  ] as const) {
    await page.mouse.move(x, y);
    await freshHud(page);
    if (
      (await page.locator("#scene").getAttribute("data-placement-valid")) ===
      "true"
    ) {
      await page.mouse.click(x, y);
      placed = true;
      break;
    }
  }
  expect(placed).toBe(true);
  await expect
    .poll(async () => (await actors(page)).some((a) => a.kind === "Barracks"))
    .toBe(true);
  // Foundations have HP, so they can already be selected while being built.
  const barracks = (await actors(page)).find((a) => a.kind === "Barracks");
  if (!barracks) throw new Error("Barracks missing");
  await page.mouse.click(barracks.x, barracks.y);
  await expect(page.locator("#selection-name")).toHaveText("Barracks");
  await expect(page.locator("#building-status")).toContainText("Complete", {
    timeout: 70_000,
  });
  await page.keyboard.press("q");
  await expect(page.locator("#training-label")).toContainText("Militia");
  await expect
    .poll(async () => (await actors(page)).some((a) => a.kind === "Militia"), {
      timeout: 35_000,
    })
    .toBe(true);
  await click(page, "Militia");
  await expect(page.locator("#unit-hp")).toContainText("40 / 40");
  await expect(page.locator("#unit-combat")).toContainText("4");
  await expect(page.locator(".unit-hp-bar")).toHaveCount(1);
  const sheep = await click(page, "Sheep", "right");
  await expect
    .poll(async () => (await actors(page)).find((a) => a.id === sheep.id)?.hp, {
      timeout: 50_000,
    })
    .toBe(0);
  await expect
    .poll(async () => (await actors(page)).find((a) => a.id === sheep.id)?.task)
    .toBe("Carcass");
  // The deer herd begins outside Town Center LOS. Keep the scout approaching
  // while its first visible deer is selected for the villager's hunt order.
  await click(page, "Scout");
  await page.locator("#move-units").click();
  await page.mouse.click(180, 360, { button: "right" });
  await expect
    .poll(
      async () =>
        (await actors(page)).some((a) => a.kind === "Deer" && a.hp > 0),
      {
        timeout: 25_000,
      },
    )
    .toBe(true);
  await click(page, "Villager");
  await expect(page.locator("#selection-name")).toHaveText("Villager");
  const deer = await click(page, "Deer", "right");
  await expect(page.locator("#selection-task")).toContainText("Hunting");
  await expect
    .poll(async () => (await actors(page)).find((a) => a.id === deer.id)?.hp, {
      timeout: 70_000,
    })
    .toBe(0);
  await expect(page.locator("#selection-task")).toContainText("Hunting");
  await expect(page.locator("#selection-load")).toContainText("food", {
    timeout: 15_000,
  });
});
