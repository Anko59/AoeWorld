import { expect, test } from "@playwright/test";
import { gameAssets } from "../game-assets.js";
import { fiveVillagerFixture } from "./fixture.js";
import { workerFixture } from "./worker-fixture.js";

for (const [edge, y] of [
  ["top", 1],
  ["bottom", 719],
] as const) {
  test(`${edge} HUD outer edge still pans at 1280 by 720 and controls work`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1280, height: 720 });
    await gameAssets(page);
    await fiveVillagerFixture(page);
    await page.goto("/");
    await expect(page.locator("#population")).toHaveText("6/10");
    const camera = () => page.locator("#minimap").getAttribute("data-camera-y");
    const before = await camera();
    await page.mouse.move(640, y);
    await expect.poll(camera).not.toBe(before);
    await page.mouse.move(640, 360);
    await page.locator("#menu-toggle").click();
    await expect(page.getByRole("dialog", { name: "Game menu" })).toBeVisible();
    await page.locator("#resume-game").click();
    await expect(page.getByRole("dialog", { name: "Game menu" })).toBeHidden();
  });
}

test("a stopped villager retains its load without counting as a wood worker", async ({
  page,
}) => {
  await gameAssets(page);
  await workerFixture(page);
  await page.goto("/");
  await expect(page.locator("#population")).toHaveText("3/5");
  await expect(page.locator("#workers-wood")).toHaveText("0");
  const worker = await page.locator("#scene").evaluate((canvas) =>
    (
      JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as {
        id: number;
        x: number;
        y: number;
      }[]
    ).find((a) => a.id === 3),
  );
  if (!worker) throw new Error("Stopped worker missing");
  await page.mouse.click(worker.x, worker.y);
  await expect(page.locator("#selection-load")).toContainText("3/10 wood");
});

test("reconnecting preserves an offscreen zero-load gold worker without terrain chunks", async ({
  page,
}) => {
  await gameAssets(page);
  await workerFixture(page);
  await page.goto("/");
  await expect(page.locator("#population")).toHaveText("3/5");
  await expect(page.locator("#workers-gold")).toHaveText("1");
  await page.reload();
  await expect(page.locator("#workers-gold")).toHaveText("1");
  await expect(page.locator("#minimap")).toHaveAttribute("data-map-hash", "");
});
