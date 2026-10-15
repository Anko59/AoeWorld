import { expect, test } from "@playwright/test";
import { gameAssets } from "../game-assets.js";
import { fiveVillagerFixture } from "./fixture.js";
import { freshHud } from "./resources.js";

test("five-villager presentation fixture supports box and double click selection at both viewport sizes", async ({
  page,
}, info) => {
  await gameAssets(page);
  await fiveVillagerFixture(page);
  await page.goto("/");
  await expect(page.locator("#population")).toHaveText("6/10");
  for (const [width, height] of [
    [1440, 900],
    [1280, 720],
  ] as const) {
    await page.setViewportSize({ width, height });
    await freshHud(page);
    const points = await page.locator("#scene").evaluate((canvas) => {
      const actors = JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as {
        kind: string;
        x: number;
        y: number;
      }[];
      return actors.filter((a) => a.kind === "Villager");
    });
    expect(points).toHaveLength(5);
    const x = Math.min(...points.map((p) => p.x)) - 12,
      y = Math.min(...points.map((p) => p.y)) - 12;
    await page.mouse.move(x, y);
    await page.mouse.down();
    await page.mouse.move(
      Math.max(...points.map((p) => p.x)) + 12,
      Math.max(...points.map((p) => p.y)) + 12,
      { steps: 4 },
    );
    await page.mouse.up();
    await expect(page.locator("#selection")).toHaveAttribute("data-count", "5");
    await expect(page.locator("#selected-icons .unit-token")).toHaveCount(5);
    await page.screenshot({
      path: `../.cache/interface-shots/five-villagers-fixture-${info.project.name}-${width}x${height}.png`,
    });
    await page.keyboard.press("Escape");
    const first = points[0];
    if (!first) throw new Error("Fixture villager missing");
    await page.mouse.dblclick(first.x, first.y);
    await expect(page.locator("#selection")).toHaveAttribute("data-count", "5");
    await page.keyboard.press("Escape");
  }
});
