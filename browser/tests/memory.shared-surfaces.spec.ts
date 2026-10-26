import { expect, type Page } from "@playwright/test";
import { test } from "../server.js";
import { writeFile } from "node:fs/promises";
import { gameAssets } from "./game-assets.js";
import { activateSyntheticMap } from "./synthetic-map.js";
import { processMemory, type ProcessMemory } from "./memory/process.js";
import { forceCanvas } from "./rendering/backends.js";
import { coverageProfile, profileCoverage } from "./memory/coverage-profile.js";

// Frame capture adds compositor readbacks and retained image buffers to the
// workload being measured. Keep command/memory JSON, as in exploration.
test.use({ screenshot: "off", trace: "off" });

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

interface RendererMemory {
  backend: string;
  renderer: [string, number][];
}

async function rendererMemory(page: Page) {
  return page.evaluate(async () => {
    const module = (await import(
      new URL("/pkg/aoe_client.js", location.href).href
    )) as { client_memory_snapshot(): string };
    return JSON.parse(module.client_memory_snapshot()) as RendererMemory;
  });
}

test("dense forest resize and camera cycles keep backing and WASM memory bounded", async ({
  playwright,
  browserName,
  launchOptions,
  headless,
  channel,
  request,
}, testInfo) => {
  test.setTimeout(120_000);
  expect((await request.post("/maps/reset")).status()).toBe(204);
  // VmRSS covers the whole browser, including its shared GPU process. A new
  // context in the worker's browser still counts driver allocations retained
  // by earlier WebGL/WebGPU tests. Isolate this workload's process, preserving
  // the project's renderer flags and every backing, RSS and WASM limit.
  const browser = await playwright[browserName].launch({
    ...launchOptions,
    headless,
    channel,
  });
  try {
    const context = await browser.newContext({
      viewport: { width: 3840, height: 2160 },
      deviceScaleFactor: 2,
    });
    const session = await browser.newBrowserCDPSession();
    const page = await context.newPage();
    await profileCoverage(page);
    await gameAssets(page, true);
    if (testInfo.project.name === "canvas") {
      await forceCanvas(page);
    }
    await page.goto("/?play=1");
    await expect(page.locator("#playground")).toHaveAttribute(
      "data-assets",
      "aoe2-local",
    );
    if (testInfo.project.name !== "browser-defaults")
      await expect(page.locator("#playground")).toHaveAttribute(
        "data-renderer",
        testInfo.project.name === "canvas" ? "canvas2d" : "webgpu",
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
    const maximumBackingRenderer = await rendererMemory(page);
    await coverageProfile(page, true);
    // Check the maximum-size backing once, then exercise allocation reuse at
    // ordinary window sizes so the software renderer stays within the suite budget.
    await page.setViewportSize({ width: 800, height: 450 });
    await activateSyntheticMap(page, true);
    await page.mouse.move(400, 225);
    await page.mouse.wheel(0, 5_000);
    await frames(page, 2);
    const samples: number[] = [];
    const residentSamples: ProcessMemory[] = [];
    const rendererSamples: RendererMemory[] = [];
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
      rendererSamples.push(await rendererMemory(page));
      await writeFile(
        testInfo.outputPath("coverage-profile.json"),
        JSON.stringify(await coverageProfile(page)),
      );
    }
    // Allow allocator warm-up, then reject continuing linear-memory growth.
    const steady = samples.slice(2);
    const residentBytes = residentSamples.map((sample) => sample.residentBytes);
    const residentSteady = residentBytes.slice(2);
    const coverageBytes = [maximumBackingRenderer, ...rendererSamples].map(
      (memory) =>
        memory.backend === "webgl2"
          ? (memory.renderer.find(
              ([name]) => name === "gpu_texture_bytes",
            )?.[1] ?? NaN) -
            128 * 1024 * 1024
          : 0,
    );
    const evidence = testInfo.outputPath("dense-forest-memory.json");
    await writeFile(
      evidence,
      JSON.stringify({
        backing,
        isolation: "Fresh browser process with the project's launch options",
        maximumBackingMemory,
        maximumBackingRenderer,
        rendererSamples,
        coverageProfile: await coverageProfile(page),
        webglCoverageBytes: coverageBytes,
        rssBudgetBytes: 1024 * 1024 * 1024,
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
    // Account the mask and depth even though they receive no RSS allowance.
    // Above one megapixel, half-size axes need only ceil-rounding headroom.
    for (const coverage of coverageBytes) {
      expect(coverage).toBeGreaterThanOrEqual(0);
      expect(coverage).toBeLessThanOrEqual((1_048_576 + 4096) * 5);
    }
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
    const commands = await coverageProfile(page);
    expect(commands.synchronous).toBe(0);
    // Initial ordinary size, then two sizes per cycle; no frame churn.
    expect(
      Object.values(commands.allocations).reduce(
        (sum, count) => sum + count,
        0,
      ),
    ).toBeLessThanOrEqual(13);
  } finally {
    await browser.close();
  }
});
