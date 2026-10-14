// Captures the views of one side, one map and one renderer (plan.json, written
// by `aoe-harness compare`). One browser page is a spectator of the one app at
// plan.url, which has plan.map_hash open from its start (AOE_OPEN_MAP_PACKAGE):
// no controller, no lease, no minimap clicks, no wheel. Each view is a camera
// fragment (`#x=…&y=…&zoom=…&renderer=…`, web/navigation.js). The page is polled
// until the settle rule holds (same rule as settle.rs: nothing pending and the
// frame unchanged for the window, 1.5 s to 4 s by frame time), or a chunk has
// failed for 12 s (every chunk still missing failed), then the read
// back camera, map and renderer and the scene rectangles go to <name>.json, and
// the screenshot to <name>.png. The checks are in Rust (capture.rs).
import { chromium } from "playwright";
import { readFileSync, writeFileSync } from "node:fs";

const plan = JSON.parse(readFileSync("plan.json", "utf8"));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
// Same software WebGPU flags as the `webgpu` project in browser/playwright.config.ts.
const WEBGPU_ARGS = [
  "--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--enable-unsafe-webgpu",
  "--ignore-gpu-blocklist", "--enable-gpu", "--enable-features=Vulkan",
  "--use-vulkan=swiftshader", "--use-webgpu-adapter=swiftshader", "--disable-vulkan-surface",
];
// The `browser-defaults` project's launch, and what `forceWebGl` does in
// browser/tests/rendering/backends.ts: no navigator.gpu, so WebGL2 is the backend.
const WEBGL2_ARGS = [];
const CAMERA_TILES = 0.01;
const CAMERA_ZOOM = 0.001;

const errors = [];

// One poll of the page, stamped with the time it was taken. A missing
// attribute reads as -1 (never settled).
async function sample(page) {
  const at = Date.now();
  const read = await page.evaluate(() => {
    const num = (v) => (v === null || v === "" ? -1 : Number(v));
    const scene = document.getElementById("scene");
    const minimap = document.getElementById("minimap");
    const camera = minimap?.dataset.cameraX === undefined ? null : {
      x: Number(minimap.dataset.cameraX), y: Number(minimap.dataset.cameraY), zoom: Number(minimap.dataset.zoom),
    };
    return {
      pending: num(scene?.getAttribute("data-chunks-pending")),
      failed: num(scene?.getAttribute("data-chunks-failed")),
      frame: num(scene?.getAttribute("data-frame")),
      camera,
      map_hash: minimap?.dataset.mapHash ?? null,
      renderer: document.getElementById("playground")?.dataset.renderer ?? null,
    };
  });
  return { at, ...read };
}

// The settle window, as settle.rs decides it: 1.5 s, or four times the median
// of the last five frame times (gaps between frame changes of at most 1 s),
// capped at 4 s.
const QUIET_MS = 1500;
const QUIET_MAX_MS = 4000;
const FRAME_INTERVALS = 4;
const IDLE_MS = 1000;
const FRAME_SAMPLES = 5;
const FAILED_MS = 12000;
function windowMs(history) {
  const frameTimes = [];
  let previous = null;
  for (let i = 1; i < history.length; i++) {
    if (history[i].frame === history[i - 1].frame) continue;
    if (previous !== null && history[i].at - previous <= IDLE_MS) frameTimes.push(history[i].at - previous);
    previous = history[i].at;
  }
  const recent = frameTimes.slice(-FRAME_SAMPLES).sort((a, b) => a - b);
  if (recent.length === 0) return QUIET_MS;
  const middle = recent.length >> 1;
  const median = recent.length % 2 === 1 ? recent[middle] : Math.floor((recent[middle - 1] + recent[middle]) / 2);
  return Math.min(QUIET_MAX_MS, Math.max(QUIET_MS, FRAME_INTERVALS * median));
}
// How long every chunk still missing has failed (failed equals pending, above
// zero), as settle.rs measures it; 0 when that is not the case now.
function failingFor(history) {
  const failing = (s) => s.failed > 0 && s.failed === s.pending;
  const last = history.at(-1);
  if (!last || !failing(last)) return 0;
  let start = history.length - 1;
  while (start > 0 && failing(history[start - 1])) start--;
  return last.at - history[start].at;
}
// Nothing pending, and the frame unchanged for the window.
function quiet(history) {
  const last = history.at(-1);
  if (!last || last.pending !== 0) return false;
  let start = history.length - 1;
  while (start > 0 && history[start - 1].pending === 0 && history[start - 1].frame === last.frame) start--;
  return last.at - history[start].at >= windowMs(history);
}
// The camera must also be where the fragment put it.
function landed(history, view) {
  const c = history.at(-1)?.camera;
  return Boolean(c) && Math.abs(c.x - view.x) <= CAMERA_TILES && Math.abs(c.y - view.y) <= CAMERA_TILES
    && Math.abs(c.zoom / view.zoom - 1) <= CAMERA_ZOOM;
}

// The rectangles of the scene and of the HUD over it (rounded outward to pixels).
async function rectangles(page) {
  return page.evaluate(() => {
    const canvas = document.getElementById("scene");
    const box = (r) => ({ x: Math.max(0, Math.floor(r.left)), y: Math.max(0, Math.floor(r.top)),
      w: Math.ceil(r.right) - Math.max(0, Math.floor(r.left)), h: Math.ceil(r.bottom) - Math.max(0, Math.floor(r.top)) });
    const scene = canvas.getBoundingClientRect();
    const masks = [];
    for (const el of document.body.querySelectorAll("*")) {
      if (el === canvas || el.contains(canvas)) continue;
      const style = getComputedStyle(el);
      if (!["fixed", "absolute"].includes(style.position) || style.display === "none" || style.visibility === "hidden") continue;
      const r = el.getBoundingClientRect();
      if (r.width < 1 || r.height < 1 || r.right <= scene.left || r.left >= scene.right || r.bottom <= scene.top || r.top >= scene.bottom) continue;
      if (el.parentElement?.closest("#minimap, #status, #map-creator") && !el.matches("#minimap, #status, #map-creator")) continue;
      masks.push(box(r));
    }
    return { scene: box(scene), masks };
  });
}

async function view(page, index, item) {
  const started = Date.now();
  if (index > 0) await page.evaluate((fragment) => { location.hash = fragment; }, item.fragment);
  const history = [];
  const deadline = started + plan.view_timeout_ms;
  for (;;) {
    history.push(await sample(page));
    if ((quiet(history) && landed(history, item)) || failingFor(history) >= FAILED_MS || Date.now() > deadline) break;
    await sleep(plan.poll_ms);
  }
  const last = history.at(-1);
  // A view with chunks still failed at the close is refused by settle.rs: no screenshot.
  const isSettled = quiet(history) && landed(history, item) && last.failed === 0;
  const parts = isSettled ? await rectangles(page) : {};
  if (isSettled) await page.screenshot({ path: `${item.name}.png`, caret: "hide", animations: "disabled" });
  writeFileSync(`${item.name}.json`, JSON.stringify({
    name: item.name, history, camera: last.camera, map_hash: last.map_hash,
    renderer: last.renderer, settle_ms: Date.now() - started, ...parts, page_errors: errors.splice(0),
  }));
}

const browser = await chromium.launch({ args: plan.renderer === "webgpu" ? WEBGPU_ARGS : WEBGL2_ARGS });
const context = await browser.newContext({
  viewport: { width: plan.viewport[0], height: plan.viewport[1] },
  deviceScaleFactor: 1,
  serviceWorkers: "block",
});
if (plan.renderer === "webgl2") {
  await context.addInitScript(() => { Object.defineProperty(navigator, "gpu", { value: undefined }); });
}
const page = await context.newPage();
page.on("pageerror", (e) => errors.push(e.message));
try {
  await page.goto(`${plan.url}#${plan.views[0].fragment}`, { waitUntil: "load", timeout: 60_000 });
  await page.waitForFunction(() => document.querySelector("#connection")?.textContent === "connected", null, { timeout: 60_000 });
  await page.waitForFunction(() => document.querySelector("#minimap")?.dataset.mapHash, null, { timeout: 60_000 });
  for (const [index, item] of plan.views.entries()) {
    try {
      await view(page, index, item);
    } catch (error) {
      writeFileSync(`${item.name}.json`, JSON.stringify({ name: item.name, error: String(error.message ?? error), page_errors: errors.splice(0) }));
    }
  }
} catch (error) {
  for (const item of plan.views) {
    writeFileSync(`${item.name}.json`, JSON.stringify({ name: item.name, error: `the page did not start: ${error.message ?? error}`, page_errors: errors.splice(0) }));
  }
}
await context.close();
await browser.close();
process.exit(0);
