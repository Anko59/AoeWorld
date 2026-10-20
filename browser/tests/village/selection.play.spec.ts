import { PNG } from "pngjs";
import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { forceCanvas } from "../rendering/backends.js";
import { fiveVillagerFixture } from "./fixture.js";
import { freshHud } from "./fixtures/resources.js";

test("five-villager presentation fixture supports box and double click selection at both viewport sizes", async ({
  page,
}, info) => {
  // Start at the first audited size, avoiding an extra native atlas redraw.
  await page.setViewportSize({ width: 1440, height: 900 });
  await gameAssets(page);
  await fiveVillagerFixture(page);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
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

test("selection ellipse remains visible through the complete zoom range", async ({
  page,
}, info) => {
  await gameAssets(page);
  if (info.project.name === "canvas") await forceCanvas(page);
  await fiveVillagerFixture(page);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
  await expect(page.locator("#population")).toHaveText("6/10");
  for (const zoom of [0.25, 0.5, 0.75, 1, 1.5, 2, 3]) {
    await page.evaluate((zoom) => {
      location.hash = `x=8188.5&y=8196.5&zoom=${zoom}`;
    }, zoom);
    await freshHud(page);
    const actor = await page.locator("#scene").evaluate((canvas) => {
      return (
        JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as {
          id: number;
          x: number;
          y: number;
        }[]
      ).find((actor) => actor.id === 2);
    });
    if (!actor) throw new Error("Fixture villager missing");
    await page.mouse.click(actor.x, actor.y);
    await expect(page.locator("#selection")).toHaveAttribute("data-count", "1");
    const image = PNG.sync.read(
      await page.screenshot({
        path: `../.cache/interface-shots/ellipse-${info.project.name}-${zoom}.png`,
      }),
    );
    let ringPixels = 0;
    for (
      let y = Math.max(0, Math.floor(actor.y - 30));
      y < Math.min(image.height, actor.y + 30);
      y++
    ) {
      for (
        let x = Math.max(0, Math.floor(actor.x - 80));
        x < Math.min(image.width, actor.x + 80);
        x++
      ) {
        const at = (y * image.width + x) * 4;
        if (
          Math.abs((image.data[at] ?? 0) - 242) < 2 &&
          Math.abs((image.data[at + 1] ?? 0) - 217) < 2 &&
          Math.abs((image.data[at + 2] ?? 0) - 89) < 2
        )
          ringPixels++;
      }
    }
    expect(ringPixels).toBeGreaterThan(12);
  }
});
