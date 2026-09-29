import { expect, test, type Page } from "@playwright/test";
import { gameAssets } from "./game-assets.js";
import { activateSyntheticMap } from "./synthetic-map.js";

async function frames(page: Page, count: number) {
  await page.evaluate(async (remaining) => {
    for (let index = 0; index < remaining; index++)
      await new Promise<void>((resolve) =>
        requestAnimationFrame(() => resolve()),
      );
  }, count);
}

async function wasmBytes(page: Page) {
  return page.evaluate(async () => {
    const module = (await import(
      new URL("/pkg/aoe_client.js", location.href).href
    )) as { default(): Promise<{ memory: WebAssembly.Memory }> };
    return (await module.default()).memory.buffer.byteLength;
  });
}

test("dense forest resize and camera cycles keep backing and WASM memory bounded", async ({
  browser,
  request,
}, testInfo) => {
  test.setTimeout(120_000);
  expect((await request.post("/maps/reset")).status()).toBe(204);
  const context = await browser.newContext({
    viewport: { width: 3840, height: 2160 },
    deviceScaleFactor: 2,
  });
  try {
    const page = await context.newPage();
    await gameAssets(page, true);
    if (testInfo.project.name === "canvas") {
      await page.addInitScript(() => {
        Object.defineProperty(navigator, "gpu", { value: undefined });
      });
    }
    await page.goto("/");
    await expect(page.locator("#playground")).toHaveAttribute(
      "data-assets",
      "aoe2-local",
    );
    await frames(page, 1);
    const backing = await page.locator("#scene").evaluate((element) => {
      const canvas = element as HTMLCanvasElement;
      return [canvas.width, canvas.height];
    });
    expect((backing[0] ?? 0) * (backing[1] ?? 0)).toBeLessThanOrEqual(
      4_194_304,
    );
    expect(Math.max(...backing)).toBeLessThanOrEqual(4096);
    // Check the maximum-size backing once, then exercise allocation reuse at
    // ordinary window sizes so the software renderer stays within the suite budget.
    await page.setViewportSize({ width: 800, height: 450 });
    await activateSyntheticMap(page, true);
    await page.mouse.move(400, 225);
    await page.mouse.wheel(0, 5_000);
    await frames(page, 2);
    const samples: number[] = [];
    for (let cycle = 0; cycle < 6; cycle++) {
      for (const key of ["ArrowRight", "ArrowDown", "ArrowLeft", "ArrowUp"]) {
        await page.keyboard.press(key);
        await frames(page, 1);
      }
      await page.setViewportSize({ width: 640, height: 360 });
      await frames(page, 2);
      await page.setViewportSize({ width: 800, height: 450 });
      await page.mouse.move(400, 225);
      await frames(page, 2);
      samples.push(await wasmBytes(page));
    }
    // Allow allocator warm-up, then reject continuing linear-memory growth.
    const steady = samples.slice(2);
    expect(Math.max(...steady) - Math.min(...steady)).toBeLessThanOrEqual(
      16 * 1024 * 1024,
    );
    await testInfo.attach("dense-forest-memory.json", {
      body: JSON.stringify({
        backing,
        wasmBytes: samples,
        limitation:
          "WASM allocation regression; excludes browser and GPU process RSS",
      }),
      contentType: "application/json",
    });
    await expect(page.locator("#connection")).toHaveText("connected");
  } finally {
    await context.close();
  }
});
