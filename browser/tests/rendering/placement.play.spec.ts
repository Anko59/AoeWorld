import { expect } from "@playwright/test";
import { PNG } from "pngjs";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { fiveVillagerFixture } from "../village/fixtures/village.js";
import { freshHud } from "../village/fixtures/resources.js";
import { arm } from "../village/fixtures/placement.js";
import { forceWebGl } from "./backends.js";

for (const requested of ["preferred", "webgl2"] as const) {
  test(`${requested} placement draws translucent native sprite and invalid red tint`, async ({
    page,
  }, info) => {
    const backend =
      requested === "webgl2" || info.project.name !== "webgpu"
        ? "webgl2"
        : "webgpu";
    if (backend === "webgl2") await forceWebGl(page);
    await gameAssets(page, true, [[2223, 3]]);
    await fiveVillagerFixture(page);
    await arm(page);
    await expect(page.locator("#playground")).toHaveAttribute(
      "data-renderer",
      backend,
    );
    const verify = async (valid: boolean, point: { x: number; y: number }) => {
      await page.mouse.move(point.x, point.y);
      await expect(page.locator("#scene")).toHaveAttribute(
        "data-placement-valid",
        String(valid),
      );
      await freshHud(page);
      const ghost = PNG.sync.read(await page.screenshot());
      await page.keyboard.press("Escape");
      await freshHud(page);
      const ground = PNG.sync.read(await page.screenshot());
      // Match half-opacity compositing of the fixture's own blue sprite,
      // including its invalid tint, against the same pixels without a ghost.
      const source = valid ? [65, 145, 245] : [195, 29, 49];
      let matching = 0,
        changed = 0;
      for (let y = point.y - 90; y < point.y + 60; y++)
        for (let x = point.x - 100; x < point.x + 100; x++) {
          const i = (y * ghost.width + x) * 4;
          if (
            [0, 1, 2].every(
              (c) =>
                Math.abs((ghost.data[i + c] ?? 0) - (ground.data[i + c] ?? 0)) <
                3,
            )
          )
            continue;
          changed++;
          if (
            source.every(
              (v, c) =>
                Math.abs(
                  (ghost.data[i + c] ?? 0) -
                    (v + (ground.data[i + c] ?? 0)) / 2,
                ) < 4,
            )
          )
            matching++;
        }
      expect(changed).toBeGreaterThan(30);
      expect(matching).toBeGreaterThan(changed * 0.8);
    };
    await verify(true, { x: 1000, y: 240 });
    await page.locator("#build-menu").click();
    await page.locator("#build-house").click();
    await verify(false, { x: 640, y: 380 });
    await expect(page.locator("#building-preview polygon")).toHaveCount(0);
  });
}
