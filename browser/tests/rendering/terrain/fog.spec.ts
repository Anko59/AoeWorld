import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import {
  BACKENDS,
  around,
  evidence,
  expectView,
  luminance,
  mean,
  open,
  ready,
  screen,
  shot,
  stableShot,
  terrainServer,
} from "./world.js";

// Characterization for the terrain cache: ground shading must follow the fog
// every time the fog changes, and end as a fresh client draws the same state.
for (const backend of BACKENDS) {
  test(`${backend} ground shading follows the fog a walking unit reveals`, async ({
    page,
  }) => {
    test.setTimeout(150_000);
    await gameAssets(page, true);
    const start: [number, number] = [8189.5, 8195.5];
    const server = await terrainServer(page, { scout: start, sight: 4 });
    await open(page, backend);
    // Ground beside the start and beside the end of the walk, clear of the
    // unit's sprite, and ground more than eight tiles from the whole walk.
    const behind = around(screen(start[0] - 1, start[1] + 1));
    const ahead = around(screen(start[0] + 7, start[1] - 7));
    const unseen = around(screen(8186.5, 8186.5));
    const light = (image: Buffer, box: typeof behind) =>
      luminance(mean(image, box));

    await expect
      .poll(async () => light(await shot(page), behind), { timeout: 20_000 })
      .toBeGreaterThan(30);
    const before = await stableShot(page);
    const visible = light(before, behind);
    expect(visible, "ground in sight is lit").toBeGreaterThan(30);
    expect(light(before, ahead), "ground not yet seen is black").toBeLessThan(
      8,
    );
    expect(light(before, unseen)).toBeLessThan(8);

    // Twelve ticks east across the screen, half a tile on each axis per tick.
    for (let step = 0; step < 12; step++) {
      server.world.tick += 1;
      server.world.scout = [
        server.world.scout[0] + 0.5,
        server.world.scout[1] - 0.5,
      ];
      server.push();
      await page.waitForTimeout(50);
    }
    await expect
      .poll(async () => light(await shot(page), ahead), { timeout: 20_000 })
      .toBeGreaterThan(30);
    const after = await stableShot(page);
    expect(light(after, ahead), "revealed ground is lit").toBeGreaterThan(30);
    const explored = light(after, behind) / visible;
    expect(explored, "ground left behind is dimmed").toBeLessThan(0.62);
    expect(explored, "ground left behind stays explored").toBeGreaterThan(0.3);
    expect(light(after, unseen), "unseen ground stays black").toBeLessThan(8);

    // A fresh client receives the final state at once. Only the walker's own
    // sprite may differ (its heading comes from the walk).
    await page.reload();
    await ready(page, backend);
    const end = screen(start[0] + 6, start[1] - 6);
    await expectView(
      page,
      after,
      `${backend} fog after the walk on a fresh client`,
      {
        x: Math.round(end.x) - 40,
        y: Math.round(end.y) - 60,
        width: 80,
        height: 85,
      },
    );
    await evidence(page, `fog-${backend}`);
  });
}
