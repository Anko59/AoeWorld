import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { building, uint } from "../../village/input/fixture.js";
import {
  BACKENDS,
  around,
  changedView,
  changes,
  evidence,
  expectView,
  greenness,
  mean,
  open,
  presented,
  ready,
  screen,
  stableShot,
  terrainServer,
} from "./world.js";

// Characterization for the terrain cache: the ground under a building is part
// of the terrain, so it must change with the building and leave with it.
for (const backend of BACKENDS) {
  test(`${backend} ground follows a placed, removed, farmed and destroyed building`, async ({
    page,
  }) => {
    test.setTimeout(180_000);
    await gameAssets(page, true);
    const server = await terrainServer(page, { scout: [8186.5, 8192.5] });
    await open(page, backend);
    await expect.poll(() => presented(page)).toBeGreaterThan(0);
    const bare = await stableShot(page);
    // The front apron tile of the house, below its footprint on the screen.
    const apron = around(screen(8194.5, 8194.5));
    const field = around(screen(8192.5, 8192.5));
    const grass = greenness(mean(bare, apron));
    expect(grass, "open ground is grass").toBeGreaterThan(15);
    expect(greenness(mean(bare, field))).toBeGreaterThan(15);

    // A House foundation clears its footprint and a one-tile apron.
    server.world.tick += 1;
    server.world.buildings = [building(20, 1, 8192, 8192, 2, 100, false)];
    server.push();
    const house = await changedView(page, bare, `${backend} house placed`);
    expect(changes(house, bare)).toBeGreaterThan(2000);
    expect(
      greenness(mean(house, apron)),
      "the apron of a foundation is cleared ground",
    ).toBeLessThan(grass - 20);
    await page.reload();
    await ready(page, backend);
    await expectView(page, house, `${backend} house on a fresh client`);

    // Removing it gives the grass back, pixel for pixel.
    server.world.tick += 1;
    server.world.buildings = [];
    server.push();
    await expectView(page, bare, `${backend} house removed`);

    // A working Farm ploughs its three by three field.
    server.world.tick += 1;
    server.world.buildings = [
      building(21, 5, 8191, 8191, 3, 900, true, 0, 175),
    ];
    server.push();
    const farm = await changedView(page, bare, `${backend} farm completed`);
    expect(changes(farm, bare)).toBeGreaterThan(2000);
    expect(
      greenness(mean(farm, field)),
      "a working farm replaces the grass of its field",
    ).toBeLessThan(grass - 20);

    // Destroyed with food left: the building list changes at once, while the
    // field belongs to the rubble and leaves only when the rubble expires.
    server.world.tick += 1;
    server.world.buildings = [];
    const destroyed = server.world.tick;
    server.frame((revision) => [
      2,
      ...revision,
      ...uint(destroyed),
      0,
      0,
      0,
      0,
      1,
      21,
    ]);
    const rubble = await changedView(page, farm, `${backend} farm destroyed`);
    expect(
      changes(rubble, bare),
      "the field of a destroyed farm stays while its rubble lasts",
    ).toBeGreaterThan(5000);
    expect(greenness(mean(rubble, field))).toBeLessThan(grass - 20);
    console.log(
      `TERRAIN GROUND ${backend}: ${JSON.stringify({
        housePixels: changes(house, bare),
        farmPixels: changes(farm, bare),
        rubblePixels: changes(rubble, bare),
        rubbleFieldGreenness: greenness(mean(rubble, field)),
        grass,
      })}`,
    );
    server.world.tick += 200;
    server.push();
    await expectView(page, bare, `${backend} farm rubble expired`);
    await evidence(page, `ground-${backend}`);
  });
}
