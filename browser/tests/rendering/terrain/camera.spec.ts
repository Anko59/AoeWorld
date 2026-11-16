import { expect, type Page } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { activateSyntheticMap } from "../../synthetic-map.js";
import { fiveVillagerFixture } from "../../village/fixtures/village.js";
import {
  BACKENDS,
  CAMERA,
  CLIP,
  changedView,
  changes,
  evidence,
  expectView,
  look,
  luminance,
  mean,
  open,
  presented,
  ready,
  shot,
  stableShot,
} from "./world.js";

/** A twentieth of the compared pixels: far more than the sprites cover. */
const REDRAWN = CLIP.width * CLIP.height * 0.05;

async function redrawn(page: Page, previous: Buffer, label: string) {
  await expect
    .poll(async () => changes(await shot(page), previous), {
      message: `${label}: changed pixels`,
      timeout: 20_000,
    })
    .toBeGreaterThan(REDRAWN);
  return stableShot(page);
}

// Characterization for the terrain cache: a new camera or the grid switch is a
// new terrain picture. Cliffs, a ramp, water, a Town Center and its villagers
// are in view, so stale ground or a wrong depth order changes pixels.
for (const backend of BACKENDS) {
  test(`${backend} terrain follows a camera pan and the grid switch`, async ({
    page,
  }) => {
    test.setTimeout(180_000);
    await gameAssets(page, true);
    await fiveVillagerFixture(page);
    await open(page, backend);
    await expect.poll(() => presented(page)).toBeGreaterThan(0);
    const plain = await stableShot(page);
    // The camera stays where the link put it: no Home key.
    await activateSyntheticMap(page, false, CAMERA);
    const home = await redrawn(page, plain, `${backend} map shown`);

    // 192 pixels to the right: both centres stand on flat ground.
    const away = [CAMERA[0] + 2.5, CAMERA[1] - 2.5] as const;
    await look(page, backend, away);
    const panned = await redrawn(page, home, `${backend} panned`);
    await look(page, backend, CAMERA);
    await expectView(page, home, `${backend} panned back`);

    await page.keyboard.press("g");
    await expect(page.locator("#grid")).toHaveAttribute("aria-pressed", "true");
    const grid = await changedView(page, home, `${backend} grid shown`);
    expect(changes(grid, home)).toBeGreaterThan(1000);
    expect(luminance(mean(grid)), "grid lines darken the ground").toBeLessThan(
      luminance(mean(home)),
    );
    await page.keyboard.press("g");
    await expect(page.locator("#grid")).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    await expectView(page, home, `${backend} grid hidden`);

    await page.keyboard.press("g");
    await expectView(page, grid, `${backend} grid shown again`);
    await look(page, backend, away);
    const gridAway = await redrawn(page, grid, `${backend} grid panned`);
    await evidence(page, `camera-${backend}`);

    // A fresh client at the panned camera draws both pictures from nothing.
    await page.reload();
    await ready(page, backend);
    await activateSyntheticMap(page, false, CAMERA);
    await expectView(page, panned, `${backend} panned on a fresh client`);
    await page.keyboard.press("g");
    await expect(page.locator("#grid")).toHaveAttribute("aria-pressed", "true");
    await expectView(page, gridAway, `${backend} grid on a fresh client`);
  });
}
