import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { bluePixels } from "../play-pixels.js";

test("WebGPU renders when the preferred BGRA canvas presents blank", async ({
  page,
  request,
}, info) => {
  test.skip(info.project.name !== "webgpu", "requires a real WebGPU adapter");
  test.setTimeout(90_000);
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
  await page.addInitScript(() => {
    // Firefox 157/Wayland/PRIME advertises BGRA even when its canvas stays
    // black. RGBA renders on the same adapter. Keep real GPU shader pixels;
    // suppress only the broken format's submissions to reproduce the failure.
    const gpu = Reflect.get(navigator, "gpu") as {
      getPreferredCanvasFormat(): string;
    };
    gpu.getPreferredCanvasFormat = () => "bgra8unorm";
    type Configuration = {
      format: string;
      device: { queue: { submit(commands: unknown): void } };
    };
    const original = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = function (
      this: HTMLCanvasElement,
      kind: string,
      ...args: unknown[]
    ) {
      const context = Reflect.apply(original, this, [kind, ...args]);
      if (kind === "webgpu" && context) {
        const configure = context.configure.bind(context) as (
          configuration: Configuration,
        ) => void;
        context.configure = (configuration: Configuration) => {
          const queue = configuration.device.queue;
          const submit = queue.submit.bind(queue);
          queue.submit = (commands: unknown) => {
            if (configuration.format !== "bgra8unorm") submit(commands);
          };
          sessionStorage.setItem("canvas-format", configuration.format);
          configure(configuration);
        };
      }
      return context;
    } as typeof original;
  });
  const messages: string[] = [];
  page.on("console", (message) => messages.push(message.text()));
  await page.goto("/?play=1");
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    "webgpu",
    { timeout: 60_000 },
  );
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect
    .poll(() => bluePixels(page.locator("#scene")))
    .toBeGreaterThan(10);
  expect(
    await page.evaluate(() => sessionStorage.getItem("canvas-format")),
  ).toBe("rgba8unorm");
  expect(messages.join("\n")).not.toMatch(/falling back|uncaptured error/i);
});
