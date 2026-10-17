import { expect, type Page } from "@playwright/test";
import { test } from "../../server.js";
import { PNG } from "pngjs";
import { gameAssets } from "../game-assets.js";
import { activateSyntheticMap } from "../synthetic-map.js";
import { forceCanvas, forceWebGl } from "./backends.js";

// World pixels only: no toolbar, minimap, status line or centre horse.
const NEAR = { left: 150, top: 110, right: 450, bottom: 400 };
// At the far zoom the 64-tile fixture is a diamond around the screen centre.
const FAR = { left: 480, top: 300, right: 620, bottom: 420 };
type Region = typeof NEAR;

test.beforeEach(async ({ request }) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
});

/** Pixels the overlay visibly darkened, as a share of the region. */
function darkened(before: PNG, after: PNG, region: Region): number {
  let count = 0;
  for (let y = region.top; y < region.bottom; y++) {
    for (let x = region.left; x < region.right; x++) {
      const offset = (y * before.width + x) * 4;
      let drop = 0;
      for (let channel = 0; channel < 3; channel++) {
        drop +=
          (before.data[offset + channel] ?? 0) -
          (after.data[offset + channel] ?? 0);
      }
      // A third of the brightness gone: plain to see, not a faint tint.
      if (drop > 60) count++;
    }
  }
  const area = (region.right - region.left) * (region.bottom - region.top);
  return count / area;
}

async function shot(page: Page): Promise<PNG> {
  return PNG.sync.read(await page.locator("#scene").screenshot());
}

for (const backend of ["webgpu", "webgl2", "canvas2d"] as const) {
  test(`grid button shows a clear tile grid at every zoom on ${backend}`, async ({
    page,
  }, testInfo) => {
    // One project forces all three renderers; the other would only repeat two.
    test.skip(testInfo.project.name !== "webgpu", "runs in the webgpu project");
    await gameAssets(page, true);
    if (backend === "webgl2") await forceWebGl(page);
    if (backend === "canvas2d") await forceCanvas(page);
    await page.goto("/?play=1");
    await expect(page.locator("#connection")).toHaveText("connected");
    await expect(page.locator("#playground")).toHaveAttribute(
      "data-renderer",
      backend,
    );
    await activateSyntheticMap(page);
    await page.mouse.move(600, 300);
    const grid = page.getByRole("button", { name: "Toggle isometric grid" });
    await expect(grid).toHaveAttribute("aria-pressed", "false");
    let near = await shot(page);
    await expect
      .poll(async () => {
        const previous = near;
        near = await shot(page);
        return darkened(previous, near, NEAR);
      })
      .toBe(0);

    await grid.click();
    await expect(grid).toHaveAttribute("aria-pressed", "true");
    // One-pixel borders of 128 by 64 pixel tiles cover about 3% of the ground.
    let share = 0;
    await expect
      .poll(async () => (share = darkened(near, await shot(page), NEAR)))
      .toBeGreaterThan(0.015);
    expect(share).toBeLessThan(0.08);

    // G is the same toggle and keeps the pressed state in step.
    await page.keyboard.press("g");
    await expect(grid).toHaveAttribute("aria-pressed", "false");
    await expect
      .poll(async () => darkened(near, await shot(page), NEAR))
      .toBe(0);

    // Far out, lines thin instead of merging into a solid sheet.
    await page.mouse.move(640, 360);
    await page.mouse.wheel(0, 1_500);
    await page.mouse.move(600, 300);
    let far = await shot(page);
    await expect
      .poll(async () => {
        const previous = far;
        far = await shot(page);
        return (
          darkened(near, far, FAR) > 0 && darkened(previous, far, FAR) === 0
        );
      })
      .toBe(true);
    await page.keyboard.press("g");
    await expect(grid).toHaveAttribute("aria-pressed", "true");
    await expect
      .poll(async () => (share = darkened(far, await shot(page), FAR)))
      .toBeGreaterThan(0.02);
    expect(share).toBeLessThan(0.25);
    await expect(page.getByRole("alert")).toBeEmpty();
  });
}
