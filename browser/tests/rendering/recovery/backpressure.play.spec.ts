import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { bluePixels } from "../play-pixels.js";

test("WebGL2 bounds unfinished frames before uploading and resumes on completion", async ({
  page,
}) => {
  test.setTimeout(150_000);
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
        let polls = 0;
        gl.bufferSubData = ((...values: Parameters<typeof upload>) => {
          Reflect.set(window, "frame-uploads", ++uploads);
          return upload(...values);
        }) as typeof gl.bufferSubData;
        // The renderer polls its oldest fence once per draw attempt, so this
        // counts attempts made while earlier frames are still unfinished.
        gl.clientWaitSync = (...values) => {
          Reflect.set(window, "frame-polls", ++polls);
          return Reflect.get(window, "complete-frames")
            ? wait(...values)
            : gl.TIMEOUT_EXPIRED;
        };
      }
      return gl;
    } as typeof original;
  });
  await page.goto("/?play=1#renderer=webgl2");
  await expect(page.locator("#connection")).toHaveText("connected");
  await page.mouse.move(600, 300);
  const uploads = () =>
    page.evaluate(() => (Reflect.get(window, "frame-uploads") as number) ?? 0);
  const polls = () =>
    page.evaluate(() => (Reflect.get(window, "frame-polls") as number) ?? 0);
  const camera = () => page.locator("#minimap").getAttribute("data-camera-x");
  // The renderer draws only when the scene changed and its art is resident, so
  // a camera that keeps moving can starve a slow runner of further draws. Pan
  // in short holds and let the view settle: each settled view draws, or is
  // refused once two frames are unfinished.
  // Alternate direction so repeated pans cannot stop at the map edge.
  let direction = "ArrowRight";
  const pan = async () => {
    const before = await camera();
    await page.keyboard.down(direction);
    try {
      await expect.poll(camera).not.toBe(before);
    } finally {
      await page.keyboard.up(direction);
    }
    direction = direction === "ArrowRight" ? "ArrowLeft" : "ArrowRight";
  };
  // The first uploads follow the first presented frames, which a loaded
  // runner can delay past any fixed frame count.
  await expect.poll(uploads, { timeout: 60_000 }).toBeGreaterThan(0);
  // A loaded software renderer may not reach a second draw at all, so the bound
  // is asserted as a ceiling. A fast runner reaches it and must then refuse.
  const deadline = Date.now() + 30_000;
  while ((await uploads()) < 2 && Date.now() < deadline) await pan();
  const reached = await uploads();
  expect(reached).toBeLessThanOrEqual(2);
  if (reached === 2) {
    // Further scene changes reach the renderer while both frames are
    // unfinished. It must poll the fences, refuse, and upload nothing.
    const attempts = await polls();
    await expect
      .poll(
        async () => {
          await pan();
          return (await polls()) - attempts;
        },
        { timeout: 60_000 },
      )
      .toBeGreaterThan(0);
  }
  await page.evaluate(async () => {
    for (let frame = 0; frame < 12; frame++) {
      await new Promise(requestAnimationFrame);
    }
  });
  expect(await uploads()).toBe(reached);
  await page.evaluate(() => Reflect.set(window, "complete-frames", true));
  await page.keyboard.press("Home");
  await expect
    .poll(() =>
      page.evaluate(() => Reflect.get(window, "frame-uploads") as number),
    )
    .toBeGreaterThan(reached);
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
