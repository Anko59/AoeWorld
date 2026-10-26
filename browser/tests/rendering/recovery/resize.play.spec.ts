import { expect } from "@playwright/test";
import { PNG } from "pngjs";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { fiveVillagerFixture } from "../../village/fixtures/village.js";
import { freshHud } from "../../village/fixtures/resources.js";
import { arm } from "../../village/fixtures/placement.js";
import { forceWebGl } from "../backends.js";

test("WebGL2 pointer input between layout and the next frame keeps the ghost at the cursor", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1600, height: 900 });
  await forceWebGl(page);
  await gameAssets(page, true, [[2223, 3]]);
  await fiveVillagerFixture(page);
  await arm(page);
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    "webgl2",
  );
  await expect(page.locator("#scene")).toHaveJSProperty("width", 1600);
  await expect(page.locator("#scene")).toHaveJSProperty("height", 900);
  await page.evaluate(() => {
    const canvas = document.querySelector<HTMLCanvasElement>("#scene");
    if (!canvas) throw new Error("Missing canvas");
    // Deliver input after CSS layout changes, before the animation callback
    // can update the old 1600-by-900 backing store and camera viewport.
    canvas.style.width = "1280px";
    canvas.style.height = "720px";
    canvas.dispatchEvent(
      new PointerEvent("pointermove", {
        bubbles: true,
        clientX: 1000,
        clientY: 240,
      }),
    );
  });
  await expect(page.locator("#scene")).toHaveJSProperty("width", 1280);
  await expect(page.locator("#scene")).toHaveJSProperty("height", 720);
  await freshHud(page);
  const ghost = PNG.sync.read(await page.screenshot());
  await page.keyboard.press("Escape");
  await freshHud(page);
  const ground = PNG.sync.read(await page.screenshot());
  let changed = 0;
  for (let y = 150; y < 300; y++) {
    for (let x = 900; x < 1100; x++) {
      const offset = (y * ghost.width + x) * 4;
      if (
        [0, 1, 2].some(
          (channel) =>
            Math.abs(
              (ghost.data[offset + channel] ?? 0) -
                (ground.data[offset + channel] ?? 0),
            ) >= 3,
        )
      )
        changed++;
    }
  }
  expect(changed).toBeGreaterThan(30);
});

test("minimap reports the camera used by a 100 ms pan frame", async ({
  page,
}) => {
  await forceWebGl(page);
  await gameAssets(page, true);
  await fiveVillagerFixture(page);
  await page.goto("/?play=1#x=8192&y=8192&zoom=1");
  await expect(page.locator("#population")).toHaveText("6/10");
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
