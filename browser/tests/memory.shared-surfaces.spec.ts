import { expect, test, type Page } from "@playwright/test";
import { writeFile } from "node:fs/promises";
import { gameAssets } from "./game-assets.js";
import { activateSyntheticMap } from "./synthetic-map.js";
import { processMemory, type ProcessMemory } from "./memory/process.js";

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
  const session = await browser.newBrowserCDPSession();
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
    const maximumBackingMemory = await processMemory(session);
    // Check the maximum-size backing once, then exercise allocation reuse at
    // ordinary window sizes so the software renderer stays within the suite budget.
    await page.setViewportSize({ width: 800, height: 450 });
    await activateSyntheticMap(page, true);
    await page.mouse.move(400, 225);
    await page.mouse.wheel(0, 5_000);
    await frames(page, 2);
    const samples: number[] = [];
    const residentSamples: ProcessMemory[] = [];
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
      residentSamples.push(await processMemory(session));
    }
    // Allow allocator warm-up, then reject continuing linear-memory growth.
    const steady = samples.slice(2);
    const residentBytes = residentSamples.map((sample) => sample.residentBytes);
    const residentSteady = residentBytes.slice(2);
    const evidence = testInfo.outputPath("dense-forest-memory.json");
    await writeFile(
      evidence,
      JSON.stringify({
        backing,
        maximumBackingMemory,
        wasmBytes: samples,
        chromiumProcessMemory: residentSamples,
        limitation:
          "Six synthetic camera/resize cycles; summed Chromium VmRSS includes shared pages, excludes host GPU allocations and thousands-of-units qualification",
      }),
    );
    await testInfo.attach("dense-forest-memory.json", {
      path: evidence,
      contentType: "application/json",
    });
    expect(Math.max(...steady) - Math.min(...steady)).toBeLessThanOrEqual(
      16 * 1024 * 1024,
    );
    expect(
      Math.max(maximumBackingMemory.residentBytes, ...residentBytes),
    ).toBeLessThanOrEqual(1024 * 1024 * 1024);
    expect(
      Math.max(...residentSteady) - Math.min(...residentSteady),
    ).toBeLessThanOrEqual(64 * 1024 * 1024);
    await expect(page.locator("#connection")).toHaveText("connected");
  } finally {
    await context.close();
  }
});
