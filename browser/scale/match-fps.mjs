// Frame rate of the human's view in a running `make scale-match`: opens the
// game as the first human slot (a plain Hello) in the pinned Chromium with
// the real art pack, then measures requestAnimationFrame intervals around the
// village and zoomed out over it. Writes SCALE_OUT (JSON); never throws a
// result away: a failed step is recorded and the next phase still runs.
/* global process, window, document, location, performance, requestAnimationFrame */
import { chromium } from "@playwright/test";
import { writeFileSync } from "node:fs";

const base = process.env["SCALE_BASE_URL"] ?? "http://127.0.0.1:9100";
const out = process.env["SCALE_OUT"] ?? "browser.json";
const seconds = Number(process.env["SCALE_SECONDS"] ?? "240");
const recording = process.env["SCALE_RECORD_VIDEO"] === "1";
const WINDOW_SECONDS = 10;

const result = {
  version: 1,
  base,
  seconds,
  phases: [],
  errors: [],
  renderer: null,
  viewport: { width: 1280, height: 720 },
};

const save = () => writeFileSync(out, JSON.stringify(result));
const percentile = (sorted, q) =>
  sorted.length === 0
    ? 0
    : sorted[Math.min(sorted.length - 1, Math.round((sorted.length - 1) * q))];
const round = (value) => Math.round(value * 100) / 100;

function summarise(name, intervals, elapsedSeconds, windows) {
  const sorted = [...intervals].sort((a, b) => a - b);
  return {
    name,
    seconds: round(elapsedSeconds),
    frames: intervals.length,
    fps_mean: round(intervals.length / Math.max(elapsedSeconds, 0.001)),
    frame_ms_p50: round(percentile(sorted, 0.5)),
    frame_ms_p95: round(percentile(sorted, 0.95)),
    frame_ms_p99: round(percentile(sorted, 0.99)),
    frame_ms_max: round(sorted.at(-1) ?? 0),
    frames_over_33ms: intervals.filter((ms) => ms > 33.4).length,
    windows,
  };
}

async function measure(page, name, phaseSeconds) {
  await page.evaluate(() => {
    window.__frames.length = 0;
  });
  const all = [];
  const windows = [];
  const started = Date.now();
  while (Date.now() - started < phaseSeconds * 1000) {
    await page.waitForTimeout(WINDOW_SECONDS * 1000);
    const got = await page.evaluate(() => window.__frames.splice(0));
    all.push(...got);
    const span = got.reduce((sum, ms) => sum + ms, 0) / 1000;
    windows.push({
      frames: got.length,
      fps: round(got.length / Math.max(span, 0.001)),
    });
  }
  const elapsed = all.reduce((sum, ms) => sum + ms, 0) / 1000;
  result.phases.push(summarise(name, all, elapsed, windows));
  save();
}

const browser = await chromium.launch({
  args: [
    "--use-angle=swiftshader",
    "--enable-unsafe-swiftshader",
    "--enable-unsafe-webgpu",
    "--ignore-gpu-blocklist",
    "--enable-gpu",
    "--enable-features=Vulkan",
    "--use-vulkan=swiftshader",
    "--use-webgpu-adapter=swiftshader",
    "--disable-vulkan-surface",
  ],
});
let video;
try {
  const page = await browser.newPage({
    viewport: result.viewport,
    recordVideo: recording
      ? { dir: `${out}.video`, size: result.viewport }
      : undefined,
  });
  video = page.video();
  page.on("pageerror", (error) => result.errors.push(String(error.message)));
  await page.addInitScript(() => {
    window.__frames = [];
    let last = performance.now();
    const step = (now) => {
      window.__frames.push(now - last);
      last = now;
      requestAnimationFrame(step);
    };
    requestAnimationFrame(step);
  });
  await page.goto(`${base}/?play=1`);
  await page.waitForFunction(
    () => document.querySelector("#connection")?.textContent === "connected",
    undefined,
    { timeout: 120_000 },
  );
  await page.waitForSelector('#playground[data-assets="aoe2-local"]', {
    timeout: 120_000,
  });
  result.renderer = await page.getAttribute("#playground", "data-renderer");
  // Chunks and units arrive over the next seconds; give the view time to fill.
  await page.waitForTimeout(15_000);
  await page.keyboard.press("Home");
  await page.waitForTimeout(3_000);
  await measure(page, "village", seconds / 2);
  // The busiest view the human has: zoomed out over its whole village.
  await page.evaluate(() => {
    location.hash = "#zoom=0.5";
  });
  await page.keyboard.press("Home");
  await page.waitForTimeout(3_000);
  await measure(page, "village_zoomed_out", seconds / 2);
} catch (error) {
  result.errors.push(String(error instanceof Error ? error.message : error));
  result.error = result.errors.at(-1);
} finally {
  save();
  try {
    await browser.close();
    if (video) result.video = await video.path();
  } catch (error) {
    result.errors.push(String(error instanceof Error ? error.message : error));
    result.error = result.errors.at(-1);
  }
  save();
}
