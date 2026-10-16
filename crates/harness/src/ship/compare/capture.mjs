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
import { sample, quiet, landed, failingFor, FAILED_MS } from "./readiness.mjs";
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

const errors = [];

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
