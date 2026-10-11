import { expect, test, type Page } from "@playwright/test";
import { PNG } from "pngjs";
import { gameAssets } from "../game-assets.js";
import { activateSyntheticMap } from "../synthetic-map.js";
import { forceCanvas, forceWebGl } from "./backends.js";

// Fixture art is 25 by 45 pixels with its hotspot at (12, 42): three rows of
// every sprite lie below the point where it stands.
const ART = { width: 25, height: 45, anchorX: 12, anchorY: 42 };
// Original art is drawn for a 96-pixel-wide tile; the world's tile is 128.
const WORLD = 128 / 96;
const TREE_TILE = [8194, 8190] as const;
const TREE_COLOR = [235, 40, 200] as const;
const RIDER_COLOR = [65, 145, 245] as const;

test.beforeEach(async ({ request }) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
});

type Box = { left: number; top: number; right: number; bottom: number };

function colorBox(image: PNG, color: readonly number[]): Box | undefined {
  let box: Box | undefined;
  for (let y = 0; y < image.height; y++) {
    for (let x = 0; x < image.width - 280; x++) {
      const offset = (y * image.width + x) * 4;
      if (
        color.some(
          (value, channel) =>
            Math.abs((image.data[offset + channel] ?? 0) - value) > 6,
        )
      )
        continue;
      box ??= { left: x, top: y, right: x, bottom: y };
      box.left = Math.min(box.left, x);
      box.right = Math.max(box.right, x);
      box.bottom = Math.max(box.bottom, y);
    }
  }
  return box;
}

async function box(page: Page, color: readonly number[]): Promise<Box> {
  let found: Box | undefined;
  await expect
    .poll(async () => {
      found = colorBox(
        PNG.sync.read(await page.locator("#scene").screenshot()),
        color,
      );
      return found === undefined ? 0 : found.bottom - found.top + 1;
    })
    .toBeGreaterThan(ART.anchorY);
  if (!found) throw new Error("sprite colour is not on screen");
  return found;
}

for (const backend of ["webgpu", "webgl2", "canvas2d"] as const) {
  test(`trees and units keep native art scale and stand on their tile on ${backend}`, async ({
    page,
  }, testInfo) => {
    // One project forces all three renderers; the other would only repeat two.
    test.skip(testInfo.project.name !== "webgpu", "runs in the webgpu project");
    await gameAssets(page, true);
    if (backend === "webgl2") await forceWebGl(page);
    if (backend === "canvas2d") await forceCanvas(page);
    await page.goto("/");
    await expect(page.locator("#connection")).toHaveText("connected");
    await expect(page.locator("#playground")).toHaveAttribute(
      "data-renderer",
      backend,
    );
    await activateSyntheticMap(
      page,
      (x, y) => x === TREE_TILE[0] && y === TREE_TILE[1],
    );
    const scene = await page.locator("#scene").boundingBox();
    if (!scene) throw new Error("Missing game canvas");
    // The camera looks at (8192, 8192); a tile is 128 by 64 pixels at zoom 1.
    const east = TREE_TILE[0] + 0.5 - 8192;
    const south = TREE_TILE[1] + 0.5 - 8192;
    const hotspot = {
      x: scene.width / 2 + (east - south) * 64,
      y: scene.height / 2 + (east + south) * 32,
    };

    const tree = await box(page, TREE_COLOR);
    // 25 by 45 native pixels fill 33 by 60 of the 128-pixel tile's screen.
    expect(tree.right - tree.left + 1).toBeGreaterThanOrEqual(32);
    expect(tree.right - tree.left + 1).toBeLessThanOrEqual(35);
    expect(tree.bottom - tree.top + 1).toBeGreaterThanOrEqual(59);
    expect(tree.bottom - tree.top + 1).toBeLessThanOrEqual(61);
    // The hotspot is the tile centre: the art starts 16 px left and 56 px up.
    expect(
      Math.abs(tree.left - (hotspot.x - ART.anchorX * WORLD)),
    ).toBeLessThan(1.5);
    expect(Math.abs(tree.top - (hotspot.y - ART.anchorY * WORLD))).toBeLessThan(
      1.5,
    );
    // The rows below the hotspot are drawn over the nearer ground, not cut.
    expect(tree.bottom + 1 - hotspot.y).toBeGreaterThan(2.5);

    const rider = await box(page, RIDER_COLOR);
    expect(rider.right - rider.left + 1).toBeGreaterThanOrEqual(32);
    expect(rider.right - rider.left + 1).toBeLessThanOrEqual(35);
    expect(rider.bottom - rider.top + 1).toBeGreaterThanOrEqual(59);
    expect(rider.bottom - rider.top + 1).toBeLessThanOrEqual(61);
    await expect(page.getByRole("alert")).toBeEmpty();
  });
}
