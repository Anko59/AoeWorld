import { expect, type BrowserContext, type Page } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { activateSyntheticMap } from "../../synthetic-map.js";
import { forceWebGl } from "../backends.js";
import { overviewTerrain, sampleFrames, zoom } from "../fixtures/bigview.js";
import { populationFixture } from "../fixtures/population.js";
import { profileGpu, readProfile, sampleOneSubmission } from "./trace.js";

test.use({ trace: "off", screenshot: "off" });

// Informational shared-host timings. API counts are deterministic diagnostics;
// neither these samples nor SwiftShader qualify the owner's physical GPU.
for (const backend of ["webgpu", "webgl2"] as const) {
  test(`${backend} population and village frame profile`, async ({
    page: gpuPage,
    playwright,
    baseURL,
  }, info) => {
    test.setTimeout(180_000);
    const fallback =
      backend === "webgl2"
        ? await playwright.chromium.launch({ args: [] })
        : undefined;
    const context = fallback
      ? await fallback.newContext({
          baseURL,
          viewport: { width: 1280, height: 720 },
        })
      : undefined;
    const page: Page = context ? await context.newPage() : gpuPage;
    let villageContext: BrowserContext | undefined;
    try {
      if (backend === "webgl2") await forceWebGl(page);
      await profileGpu(page);
      const assets = await gameAssets(page);
      const population = await populationFixture(page);
      await page.goto("/?play=1&perf=1#x=8192.5&y=8192.5&zoom=.25");
      try {
        await expect(page.locator("#connection")).toHaveText("connected", {
          timeout: 60_000,
        });
      } catch (error) {
        console.log(
          "Startup diagnostic:",
          await page.locator("#unsupported").textContent(),
        );
        throw error;
      }
      await expect(page.locator("#playground")).toHaveAttribute(
        "data-renderer",
        backend,
      );
      await activateSyntheticMap(page, true);
      await overviewTerrain(page);
      const sample = async (target = page) => {
        await readProfile(target, true);
        const timing = await sampleFrames(target);
        return { ...timing, api: await readProfile(target) };
      };
      const sprites = await sample();
      const spriteFrame = await sampleOneSubmission(page);
      await expect(page.locator("#playground")).toHaveAttribute(
        "data-renderer",
        backend,
      );
      if (backend === "webgpu") {
        for (const operation of [
          "GPUDevice.createBuffer",
          "GPUDevice.createBindGroup",
          "GPUQueue.writeTexture",
          "GPUBuffer.mapAsync",
          "GPUDevice.createQuerySet",
        ]) {
          expect(spriteFrame[operation] ?? 0, operation).toBe(0);
        }
        expect(spriteFrame["GPUQueue.writeBuffer"]).toBe(
          spriteFrame["GPUQueue.submit"],
        );
        expect(spriteFrame.contourInstances ?? 0).toBeLessThan(
          (spriteFrame.worldInstances ?? 0) / 100,
        );
      }
      console.log(
        `SPRITE STAGE ${backend}: ${JSON.stringify({ sprites, spriteFrame })}`,
      );
      await zoom(page, 700, "markers");
      const markers = await sample();
      await zoom(page, 10000, "map");
      const map = await sample();
      const villageBrowser = fallback ?? gpuPage.context().browser();
      if (!villageBrowser) throw new Error("Benchmark browser is unavailable");
      villageContext = await villageBrowser.newContext({
        baseURL,
        viewport: { width: 1280, height: 720 },
      });
      const villagePage = await villageContext.newPage();
      await page.close();
      if (backend === "webgl2") await forceWebGl(villagePage);
      await profileGpu(villagePage);
      await gameAssets(villagePage);
      await villagePage.request.post("/maps/reset");
      await villagePage.goto("/?play=1&perf=1");
      await expect(villagePage.locator("#connection")).toHaveText("connected", {
        timeout: 60_000,
      });
      await expect(villagePage.locator("#playground")).toHaveAttribute(
        "data-renderer",
        backend,
      );
      const village = await sample(villagePage);
      const villageFrame = await sampleOneSubmission(villagePage);
      await expect(villagePage.locator("#playground")).toHaveAttribute(
        "data-renderer",
        backend,
      );
      const browserVersion = villagePage.context().browser()?.version();
      const result = {
        backend,
        browser: browserVersion,
        assets,
        population,
        residentTrees: 81 * 1024,
        viewport: [1280, 720],
        mode: "existing idle frame sampler, plus a separately profiled single cache invalidation",
        sprites,
        spriteFrame,
        markers,
        map,
        village,
        villageFrame,
      };
      console.log(`SPRITE PROFILE ${JSON.stringify(result)}`);
      await info.attach("sprite-profile", {
        body: JSON.stringify(result, null, 2),
        contentType: "application/json",
      });
    } finally {
      await villageContext?.close();
      await context?.close();
      await fallback?.close();
    }
  });
}
