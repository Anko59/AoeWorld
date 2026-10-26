import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { bluePixels } from "../play-pixels.js";

test("WebGPU device loss preserves the current camera after panning and zooming", async ({
  page,
  request,
}) => {
  test.setTimeout(90_000);
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page, true);
  await page.addInitScript(() => {
    const prototype = (
      Reflect.get(window, "GPUAdapter") as {
        prototype: {
          requestDevice(...args: unknown[]): Promise<{ destroy(): void }>;
        };
      }
    ).prototype;
    const requestDevice = prototype.requestDevice;
    prototype.requestDevice = async function (...args) {
      const device = await Reflect.apply(requestDevice, this, args);
      window.addEventListener("fail-gpu", () => device.destroy());
      return device;
    };
  });
  const fragment = "x=8192.25&y=8192.5&zoom=1.25&keep=value";
  await page.goto(`/?play=1#${fragment}`);
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    "webgpu",
    { timeout: 60_000 },
  );
  await expect(page.locator("#connection")).toHaveText("connected");
  const minimap = page.locator("#minimap");
  await expect(minimap).toHaveAttribute("data-camera-x", "8192.25");
  await expect(minimap).toHaveAttribute("data-camera-y", "8192.5");
  await expect(minimap).toHaveAttribute("data-zoom", "1.25");
  await page.mouse.move(600, 300);
  await page.keyboard.press("ArrowRight");
  await page.mouse.wheel(0, -150);
  await expect(minimap).not.toHaveAttribute("data-camera-x", "8192.25");
  await expect(minimap).not.toHaveAttribute("data-camera-y", "8192.5");
  await expect(minimap).not.toHaveAttribute("data-zoom", "1.25");
  // Capture and destroy in one task, after the HUD reflects the completed input.
  // The URL deliberately still holds the original camera, not this live view.
  expect(new URL(page.url()).hash).toBe(`#${fragment}`);
  const camera = await minimap.evaluate((element) => {
    const values = ["data-camera-x", "data-camera-y", "data-zoom"].map(
      (name) => {
        const value = element.getAttribute(name);
        if (value === null)
          throw new Error(`Missing camera attribute: ${name}`);
        return [name, value] as const;
      },
    );
    window.dispatchEvent(new Event("fail-gpu"));
    return values;
  });
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    "webgl2",
    { timeout: 60_000 },
  );
  await expect(page.locator("#connection")).toHaveText("connected");
  for (const [name, value] of camera) {
    await expect(minimap).toHaveAttribute(name, value);
  }
  expect(
    new URLSearchParams(new URL(page.url()).hash.slice(1)).get("keep"),
  ).toBe("value");
});

for (const failure of [
  "adapter",
  "queue",
  "blank",
  "rejected",
  "lost",
  "validation",
] as const) {
  test(`WebGPU ${failure} failure falls back and keeps drawing`, async ({
    page,
    request,
  }) => {
    test.setTimeout(90_000);
    expect((await request.post("/maps/reset")).status()).toBe(204);
    // Use CI's generated colours even when a private pack is installed locally.
    await gameAssets(page, true);
    const messages: string[] = [];
    page.on("console", (message) => messages.push(message.text()));
    await page.addInitScript((failure) => {
      type Device = EventTarget & {
        destroy(): void;
        queue: {
          submit(commands: unknown): void;
          onSubmittedWorkDone(): Promise<void>;
        };
      };
      type Adapter = { requestDevice(...args: unknown[]): Promise<Device> };
      const gpu = (
        navigator as unknown as {
          gpu: { requestAdapter(...args: unknown[]): Promise<Adapter | null> };
        }
      ).gpu;
      if (!gpu) throw new Error("This regression requires real WebGPU");
      const request = gpu.requestAdapter.bind(gpu);
      gpu.requestAdapter = async (...args) => {
        if (failure === "adapter") return new Promise(() => {});
        const adapter = await request(...args);
        if (!adapter) throw new Error("This regression requires an adapter");
        const deviceRequest = adapter.requestDevice.bind(adapter);
        adapter.requestDevice = async (...args) => {
          const device = await deviceRequest(...args);
          const destroy = device.destroy.bind(device);
          device.destroy = () => {
            sessionStorage.setItem("gpu-destroyed", "true");
            destroy();
          };
          let submissions = 0;
          const submit = device.queue.submit.bind(device.queue);
          const completed = device.queue.onSubmittedWorkDone.bind(device.queue);
          device.queue.submit = (commands) => {
            submissions++;
            sessionStorage.setItem("gpu-submissions", String(submissions));
            if (
              failure !== "blank" &&
              (failure !== "queue" || submissions === 1)
            )
              submit(commands);
          };
          if (failure === "queue")
            device.queue.onSubmittedWorkDone = () =>
              submissions > 1 ? new Promise(() => {}) : completed();
          if (failure === "rejected")
            device.queue.onSubmittedWorkDone = () =>
              Promise.reject(new Error("injected queue rejection"));
          window.addEventListener("fail-gpu", () => {
            if (failure === "lost") device.destroy();
            if (failure === "validation") {
              const event = new Event("uncapturederror");
              const ErrorType = (
                window as unknown as {
                  GPUValidationError: new (message: string) => unknown;
                }
              ).GPUValidationError;
              Object.defineProperty(event, "error", {
                value: new ErrorType("injected validation failure"),
              });
              device.dispatchEvent(event);
            }
          });
          return device;
        };
        return adapter;
      };
    }, failure);
    await page.goto("/?play=1");
    if (failure === "lost" || failure === "validation") {
      await expect(page.locator("#playground")).toHaveAttribute(
        "data-renderer",
        "webgpu",
        { timeout: 60_000 },
      );
      await expect
        .poll(() => bluePixels(page.locator("#scene")))
        .toBeGreaterThan(10);
      await page.evaluate(() => window.dispatchEvent(new Event("fail-gpu")));
    }
    await expect(page.locator("#playground")).toHaveAttribute(
      "data-renderer",
      "webgl2",
      { timeout: 60_000 },
    );
    await expect(page.locator("#connection")).toHaveText("connected");
    await expect
      .poll(() => bluePixels(page.locator("#scene")))
      .toBeGreaterThan(10);
    expect(
      messages.some((message) =>
        /WebGPU.*falling back to WebGL2/is.test(message),
      ),
    ).toBe(true);
    // Browser errors may include multiline stacks. Require the specific
    // captured cause as well as the automatic fallback announcement.
    const cause = {
      adapter: "WebGPU initialization timed out",
      queue: "no completed frame/upload within 5 seconds",
      blank: "startup clear",
      rejected: "WebGPU queue completion rejected",
      lost: "WebGPU device lost (Destroyed)",
      validation: "WebGPU uncaptured error",
    }[failure];
    expect(messages.join("\n")).toContain(cause);
    if (failure !== "adapter") {
      expect(
        await page.evaluate(() => sessionStorage.getItem("gpu-destroyed")),
        "every abandoned WebGPU device must be explicitly retired",
      ).toBe("true");
    }
    if (failure === "queue") {
      // The verified startup clear, then at most two unfinished game frames.
      expect(
        await page.evaluate(() =>
          Number(sessionStorage.getItem("gpu-submissions")),
        ),
      ).toBe(3);
    }
    const before = await page.locator("#minimap").getAttribute("data-camera-x");
    expect(before).not.toBeNull();
    await page.mouse.move(600, 300);
    await page.keyboard.down("ArrowRight");
    try {
      await expect
        .poll(() => page.locator("#minimap").getAttribute("data-camera-x"))
        .not.toBe(before);
    } finally {
      await page.keyboard.up("ArrowRight");
    }
    await page.keyboard.press("Home");
    await expect
      .poll(() => bluePixels(page.locator("#scene")))
      .toBeGreaterThan(10);
    await page.evaluate(() => window.dispatchEvent(new Event("pagehide")));
    expect(
      await page
        .locator("#scene")
        .evaluate((canvas) =>
          (canvas as HTMLCanvasElement).getContext("webgl2")?.isContextLost(),
        ),
      "leaving the fallback page must retire its context before browser teardown",
    ).toBe(true);
  });
}
