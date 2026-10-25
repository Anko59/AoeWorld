import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { bluePixels } from "../play-pixels.js";

test("WebGL2 bounds unfinished frames before uploading and resumes on completion", async ({
  page,
}) => {
  await gameAssets(page, true);
  await page.addInitScript(() => {
    const original = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = function (
      this: HTMLCanvasElement,
      kind: string,
      ...args: unknown[]
    ) {
      const gl = Reflect.apply(original, this, [
        kind,
        ...args,
      ]) as WebGL2RenderingContext | null;
      if (kind === "webgl2" && gl && !Reflect.get(gl, "probed")) {
        Reflect.set(gl, "probed", true);
        const upload = gl.bufferSubData.bind(gl);
        const wait = gl.clientWaitSync.bind(gl);
        let uploads = 0;
        gl.bufferSubData = ((...values: Parameters<typeof upload>) => {
          Reflect.set(window, "frame-uploads", ++uploads);
          return upload(...values);
        }) as typeof gl.bufferSubData;
        gl.clientWaitSync = (...values) =>
          Reflect.get(window, "complete-frames")
            ? wait(...values)
            : gl.TIMEOUT_EXPIRED;
      }
      return gl;
    } as typeof original;
  });
  await page.goto("/?play=1#renderer=webgl2");
  await expect(page.locator("#connection")).toHaveText("connected");
  await page.mouse.move(600, 300);
  await page.keyboard.down("ArrowRight");
  try {
    await page.evaluate(async () => {
      for (let frame = 0; frame < 12; frame++) {
        await new Promise(requestAnimationFrame);
      }
    });
  } finally {
    await page.keyboard.up("ArrowRight");
  }
  expect(await page.evaluate(() => Reflect.get(window, "frame-uploads"))).toBe(
    2,
  );
  await page.evaluate(() => Reflect.set(window, "complete-frames", true));
  await page.keyboard.press("Home");
  await expect
    .poll(() =>
      page.evaluate(() => Reflect.get(window, "frame-uploads") as number),
    )
    .toBeGreaterThan(2);
  await expect
    .poll(() => bluePixels(page.locator("#scene")))
    .toBeGreaterThan(10);
});

test("leaving the game retires its WebGPU device before browser teardown", async ({
  page,
  request,
}) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page, true);
  await page.addInitScript(() => {
    const prototype = (
      Reflect.get(window, "GPUDevice") as {
        prototype: { destroy(): void };
      }
    ).prototype;
    const destroy = prototype.destroy;
    prototype.destroy = function () {
      sessionStorage.setItem("page-device-retired", "true");
      Reflect.apply(destroy, this, []);
    };
  });
  await page.goto("/?play=1");
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    "webgpu",
  );
  await expect
    .poll(() => bluePixels(page.locator("#scene")))
    .toBeGreaterThan(10);
  await page.goto("/diagnostics.html");
  await expect(page).toHaveURL(/\/diagnostics\.html$/);
  expect(
    await page.evaluate(() => sessionStorage.getItem("page-device-retired")),
  ).toBe("true");
});
