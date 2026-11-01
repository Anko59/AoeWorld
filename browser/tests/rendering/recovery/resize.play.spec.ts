import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { fiveVillagerFixture } from "../../village/fixtures/village.js";
import { freshHud } from "../../village/fixtures/resources.js";
import { forceWebGl } from "../backends.js";

test("minimap reports the camera used by a 100 ms pan frame", async ({
  page,
}) => {
  await forceWebGl(page);
  await gameAssets(page, true);
  await fiveVillagerFixture(page);
  await page.goto("/?play=1#x=8192&y=8192&zoom=1");
  await expect(page.locator("#population")).toHaveText("6");
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    "webgl2",
  );
  await freshHud(page);
  const { camera, rendered, pan, expected } = await page.evaluate(async () => {
    const navigation = Reflect.get(window, "aoeNavigation") as {
      frame(state: Float64Array): Float64Array;
    };
    const frame = navigation.frame;
    let pan = 0;
    let expected: [number, number] = [NaN, NaN];
    // CI can reach the 100 ms frame-delta cap. At 1600 px/s this pans
    // exactly 160 px, regardless of the machine's real frame cadence.
    navigation.frame = (state) => {
      state[9] = 100;
      const output = frame(state);
      pan = output[0] ?? 0;
      expected = [(state[2] ?? NaN) + pan / 128, (state[3] ?? NaN) - pan / 128];
      return output;
    };
    document.dispatchEvent(
      new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }),
    );
    try {
      await new Promise(requestAnimationFrame);
      await new Promise(requestAnimationFrame);
      const minimap = document.getElementById("minimap");
      const position = document.getElementById("world-position");
      if (!minimap || !position) throw new Error("Missing camera readouts");
      return {
        camera: [
          Number(minimap.dataset.cameraX),
          Number(minimap.dataset.cameraY),
        ] as const,
        rendered: (position.textContent ?? "").split(",").map(Number),
        pan,
        expected,
      };
    } finally {
      document.dispatchEvent(
        new KeyboardEvent("keyup", { key: "ArrowRight", bubbles: true }),
      );
      navigation.frame = frame;
    }
  });
  expect(pan).toBe(160);
  const offset = (camera[0] - expected[0] - (camera[1] - expected[1])) * 64;
  expect(Math.abs(offset)).toBeLessThan(1.5);
  expect(rendered).toHaveLength(2);
  expect(rendered[0]).toBeGreaterThan(8192);
  for (const [axis, coordinate] of camera.entries()) {
    const actual = rendered[axis];
    if (actual === undefined) throw new Error("Missing rendered coordinate");
    // The render loop's coordinate readout rounds to one decimal place.
    expect(Math.abs(coordinate - actual)).toBeLessThan(0.051);
  }
});
