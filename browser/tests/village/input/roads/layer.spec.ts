import { expect } from "@playwright/test";
import { test } from "../../../../server.js";

test("far map draws road plots only with the Roads and bridges layer enabled", async ({
  page,
}) => {
  await page.route("**/road-layer-test", (route) =>
    route.fulfill({
      contentType: "text/html",
      body: "<canvas id='scene' width='128' height='128'></canvas>",
    }),
  );
  await page.goto("/road-layer-test");
  const result = await page.evaluate(async () => {
    // @ts-expect-error Runtime module is served by the game.
    const { initStrategic } = await import("/navigation/strategic.js");
    const strategic = initStrategic(
      { disabled: true, canvas: document.createElement("canvas"), revision: 0 },
      { records: () => [], revision: () => 0 },
    );
    const frame = [256, 256, 64, 64, 128, 128, 0.0625, 1];
    strategic.update([], [], [60, 60, 68, 68, 23]);
    strategic.frame(frame);
    const canvas =
      document.querySelector<HTMLCanvasElement>("#strategic-scene");
    if (!canvas) throw Error("Strategic canvas missing");
    const context = canvas.getContext("2d");
    if (!context) throw Error("Strategic canvas context missing");
    const pixel = () => Array.from(context.getImageData(64, 64, 1, 1).data);
    const toggle = () =>
      document.dispatchEvent(
        new KeyboardEvent("keydown", { key: "F6", bubbles: true }),
      );
    toggle();
    const hidden = pixel();
    toggle();
    const visible = pixel();
    strategic.update([], [], []);
    strategic.frame(frame);
    return { hidden, visible, removed: pixel() };
  });
  const ground = [88, 123, 69, 255];
  expect(result).toEqual({
    hidden: ground,
    visible: [214, 181, 123, 255],
    removed: ground,
  });
});
