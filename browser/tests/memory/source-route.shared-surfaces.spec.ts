import { expect, test } from "@playwright/test";
import { writeFile } from "node:fs/promises";
import { processMemory, type ProcessMemory } from "./process.js";
import { forceCanvas } from "../rendering/backends.js";

const sourceUrl = process.env["AOE_POLISH_SOURCE_URL"];
const sourceHash = process.env["AOE_POLISH_SOURCE_HASH"];

test("real-pack source route evicts terrain without continuing process memory growth", async ({
  page,
  browser,
}, testInfo) => {
  if (!sourceUrl || !sourceHash) {
    if (process.env["AOE_POLISH_SOURCE_REQUIRED"] === "1")
      throw new Error(
        "AOE_POLISH_SOURCE_URL and AOE_POLISH_SOURCE_HASH are required",
      );
    test.skip(true, "requires an activated private source map");
    return;
  }
  test.setTimeout(600_000);
  expect(sourceHash).toMatch(/^[0-9a-f]{64}$/);
  const session = await browser.newBrowserCDPSession();
  if (testInfo.project.name === "canvas") {
    await forceCanvas(page);
  }
  const chunks = new Set<string>();
  page.on("response", (response) => {
    const path = new URL(response.url()).pathname;
    if (path.startsWith(`/maps/${sourceHash}/chunks/`) && response.ok())
      chunks.add(path);
  });
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(sourceUrl);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-assets",
    "aoe2-local",
  );
  // Connection acknowledgement can precede the first unit snapshot. Home must
  // target the actual horse, not the provisional world-center camera.
  await expect(page.locator("#minimap-primary")).toHaveAttribute(
    "visibility",
    "visible",
  );
  await page.keyboard.press("Home");
  await page.waitForLoadState("networkidle");
  const origin = await page.locator("#world-position").innerText();
  await page.mouse.move(640, 360);
  await page.mouse.wheel(0, 5_000);
  const samples: { memory: ProcessMemory; cache: string; camera: string }[] =
    [];
  // Pointer capture permits long middle-button drags beyond the viewport.
  // Twelve outward regions and twelve returns exercise actual source loading
  // and the 512-chunk residency cap without changing or resetting the world.
  for (let step = 0; step < 24; step++) {
    await page.mouse.move(640, 360);
    await page.mouse.down({ button: "middle" });
    await page.mouse.move(640 + (step < 12 ? -8000 : 8000), 360);
    await page.mouse.up({ button: "middle" });
    await page.mouse.move(640, 360);
    await page.waitForLoadState("networkidle");
    await expect(page.locator("#connection")).toHaveText("connected");
    const cache = await page.locator("#terrain-cache").innerText();
    const residentChunks = Number(/\((\d+) \/ 512 chunks\)/.exec(cache)?.[1]);
    expect(residentChunks).toBeGreaterThan(0);
    expect(residentChunks).toBeLessThanOrEqual(512);
    samples.push({
      memory: await processMemory(session),
      cache,
      camera: await page.locator("#world-position").innerText(),
    });
  }
  await page.keyboard.press("Home");
  await expect(page.locator("#world-position")).toHaveText(origin);
  await page.waitForLoadState("networkidle");
  const evidence = testInfo.outputPath("source-route-memory.json");
  await writeFile(
    evidence,
    JSON.stringify({
      sourceHash,
      uniqueChunks: chunks.size,
      samples,
      errors,
      limitation:
        "24 source-region drags, one stationary unit, sampled summed Chromium VmRSS; excludes host GPU residency and moving-unit qualification",
    }),
  );
  await testInfo.attach("source-route-memory.json", {
    path: evidence,
    contentType: "application/json",
  });
  expect(chunks.size).toBeGreaterThan(512);
  expect(new Set(samples.map((sample) => sample.camera)).size).toBeGreaterThan(
    12,
  );
  expect(errors).toEqual([]);
  const residentBytes = samples.map((sample) => sample.memory.residentBytes);
  expect(Math.max(...residentBytes)).toBeLessThanOrEqual(1024 * 1024 * 1024);
  const steady = residentBytes.slice(-4);
  expect(Math.max(...steady) - Math.min(...steady)).toBeLessThanOrEqual(
    64 * 1024 * 1024,
  );
  await session.detach();
});
