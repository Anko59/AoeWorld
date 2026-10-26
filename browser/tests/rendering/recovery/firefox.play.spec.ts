import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { bluePixels } from "../play-pixels.js";

test("Firefox with WebGPU enabled displays a playable frame", async ({
  page,
  request,
  browser,
}, info) => {
  test.setTimeout(90_000);
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
  const messages: string[] = [];
  page.on("console", (message) => messages.push(message.text()));
  await page.goto("/?play=1");
  const adapter = await page.evaluate(async () => {
    const gpu = (
      navigator as unknown as {
        gpu?: {
          requestAdapter(): Promise<{
            limits: object;
            features: Iterable<string>;
          } | null>;
        };
      }
    ).gpu;
    if (!gpu) return { available: false, reason: "WebGPU API unavailable" };
    try {
      const adapter = await Promise.race([
        gpu.requestAdapter(),
        new Promise<never>((_, reject) =>
          setTimeout(() => reject(new Error("Adapter probe timed out")), 5_000),
        ),
      ]);
      return adapter
        ? { available: true, features: Array.from(adapter.features) }
        : { available: false, reason: "No headless Firefox adapter" };
    } catch (error) {
      return { available: false, reason: String(error) };
    }
  });
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await expect
    .poll(() => bluePixels(page.locator("#scene")))
    .toBeGreaterThan(10);
  const renderer = await page
    .locator("#playground")
    .getAttribute("data-renderer");
  if (adapter.available) {
    expect(renderer).toBe("webgpu");
    expect(messages.join("\n")).toContain("canvas Rgba8Unorm");
    expect(messages.join("\n")).not.toMatch(/falling back|uncaptured error/i);
  }
  await info.attach("firefox-webgpu.png", {
    contentType: "image/png",
    body: await page.screenshot(),
  });
  console.log(
    JSON.stringify({ browser: browser.version(), adapter, renderer, messages }),
  );
  await info.attach("firefox-webgpu", {
    contentType: "application/json",
    body: JSON.stringify({
      adapter,
      browser: browser.version(),
      renderer,
      messages,
    }),
  });
});
