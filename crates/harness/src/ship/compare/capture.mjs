// Captures the views of one side of a comparison (plan.json, written by
// `aoe-harness compare`): one browser, one session, the one app at plan.url.
// The session is the gameplay controller of a server that only this run uses,
// so there is no lease to wait out. For each view it places the camera, waits
// until the frame is settled, then writes <name>.png and <name>.json (the
// camera as read back, the scene rectangle and the HUD rectangles over it).
// Everything the page must be asked sits behind `camera` and `settle` below: a
// camera URL fragment or a "chunks loaded" attribute in the client would
// replace those two objects and nothing else.
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
const WEBGL2_ARGS = ["--use-angle=swiftshader", "--enable-unsafe-swiftshader"];
const WHEEL_RATE = 0.0015; // crates/client/src/playground_controls.rs: zoom *= exp(-delta * rate)
const TILE_TOLERANCE = 0.05;
const ZOOM_TOLERANCE = 0.002;

const errors = [];
let inflight = 0;

// --- the camera: read it, place it ------------------------------------------

// `aoeNavigation.frame` receives the camera state every frame (navigation.js):
// state[0..1] world size in tiles, [2..3] centre in tiles, [8] zoom.
const camera = {
  async instrument(page) {
    await page.evaluate(() => {
      const nav = window.aoeNavigation;
      if (!nav || nav.__compare) return;
      const frame = nav.frame;
      nav.frame = function (state) {
        window.__compareState = Array.from(state);
        return frame.call(this, state);
      };
      nav.__compare = true;
    });
  },
  async read(page) {
    return page.evaluate(() => {
      const s = window.__compareState;
      if (!s) return null;
      return { world: [s[0], s[1]], x: s[2], y: s[3], zoom: s[8] };
    });
  },
  // Zoom first (the wheel zooms around the pointer, which sits on the scene
  // centre), then the minimap click, which sets the centre directly.
  async place(page, view) {
    for (let attempt = 0; attempt < 4; attempt++) {
      const now = await this.read(page);
      if (Math.abs(Math.log(now.zoom / view.zoom)) < ZOOM_TOLERANCE) break;
      const box = await page.locator("#scene").boundingBox();
      await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
      await page.mouse.wheel(0, Math.log(now.zoom / view.zoom) / WHEEL_RATE);
      await sleep(700);
    }
    let at = null;
    for (let attempt = 0; attempt < 4; attempt++) {
      const now = await this.read(page);
      const want = [view.x, view.y];
      // A click lands where the letterboxed square minimap maps it (minimap.js);
      // aim again from the camera read back until it is where the bookmark is.
      const aim = at ? [at[0] + want[0] - now.x, at[1] + want[1] - now.y] : want;
      if (at && Math.abs(want[0] - now.x) < TILE_TOLERANCE && Math.abs(want[1] - now.y) < TILE_TOLERANCE) break;
      at = aim;
      const rect = await page.locator("#minimap-map").boundingBox();
      const side = Math.min(rect.width, rect.height);
      const left = rect.x + (rect.width - side) / 2;
      const top = rect.y + (rect.height - side) / 2;
      await page.mouse.click(left + (at[0] / now.world[0]) * side, top + (at[1] / now.world[1]) * side);
      await sleep(900);
    }
    return this.read(page);
  },
};

// --- settling ---------------------------------------------------------------

// The status line says "terrain cache: … (N / 512 chunks)". Settled: that
// count, the camera and the number of map requests in flight are unchanged for
// plan.settle_polls polls in a row, within plan.view_timeout_ms.
const settle = {
  async signature(page) {
    const text = (await page.locator("#terrain-cache").textContent()) ?? "";
    const chunks = Number(/\((\d+) \/ \d+ chunks\)/.exec(text)?.[1] ?? NaN);
    const cam = await camera.read(page);
    return { chunks, cam, inflight };
  },
  async wait(page) {
    const deadline = Date.now() + plan.view_timeout_ms;
    let last = "";
    let same = 0;
    let signature;
    do {
      await sleep(plan.poll_ms);
      signature = await this.signature(page);
      const key = JSON.stringify(signature);
      same = key === last ? same + 1 : 0;
      last = key;
      if (same >= plan.settle_polls && signature.inflight === 0 && signature.chunks > 0) return signature;
    } while (Date.now() < deadline);
    throw new Error(`not settled in ${plan.view_timeout_ms} ms (chunks ${signature.chunks}, ${signature.inflight} requests in flight)`);
  },
};

// --- the page ---------------------------------------------------------------

async function ready(page) {
  await page.waitForSelector("#creator-toggle", { timeout: 60_000 });
  await page.waitForFunction(() => document.querySelector("#connection")?.textContent === "connected", null, { timeout: 60_000 });
  await camera.instrument(page);
  await page.waitForFunction(() => window.__compareState !== undefined, null, { timeout: 30_000 });
}

async function openMap(page, hash) {
  if (await page.locator(`#minimap[data-map-hash="${hash}"]`).count()) return;
  await page.click("#creator-toggle");
  await page.selectOption("#map-package", hash);
  await page.click("#open-map");
  try {
    await page.waitForFunction(
      () => /Saved map active:/.test(document.querySelector("#map-estimate")?.textContent ?? ""),
      null, { timeout: 60_000 });
  } catch {
    const said = (await page.locator("#map-estimate").textContent().catch(() => "")) ?? "";
    throw new Error(`the map did not open (${said.trim().slice(0, 160) || "no message"}): a spectator session cannot open maps`);
  }
  await page.reload();
  await page.waitForSelector(`#minimap[data-map-hash="${hash}"]`, { timeout: 60_000 });
  await ready(page);
}

// The scene is the canvas; the HUD is every positioned element that overlaps
// it and does not contain it. Rectangles are rounded outward to whole pixels.
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
    return { scene: box(scene), masks, renderer: document.getElementById("playground")?.dataset.renderer ?? null };
  });
}

async function capture(page, view) {
  const started = Date.now();
  try {
    await openMap(page, view.map_hash);
    const placed = await camera.place(page, view);
    const settled = await settle.wait(page);
    const parts = await rectangles(page);
    await page.screenshot({ path: `${view.name}.png`, caret: "hide", animations: "disabled" });
    writeFileSync(`${view.name}.json`, JSON.stringify({
      name: view.name, camera: settled.cam, placed, chunks: settled.chunks, settle_ms: Date.now() - started, ...parts,
      page_errors: errors.splice(0),
    }));
    return true;
  } catch (error) {
    writeFileSync(`${view.name}.json`, JSON.stringify({ name: view.name, error: String(error.message ?? error), page_errors: errors.splice(0) }));
    return false;
  }
}

const browser = await chromium.launch({ args: plan.renderer === "webgpu" ? WEBGPU_ARGS : WEBGL2_ARGS });
const context = await browser.newContext({
  viewport: { width: plan.viewport.width, height: plan.viewport.height },
  deviceScaleFactor: 1,
  serviceWorkers: "block",
});
const page = await context.newPage();
page.on("pageerror", (e) => errors.push(e.message));
page.on("request", (r) => { if (new URL(r.url()).pathname.startsWith("/maps/")) inflight++; });
for (const done of ["requestfinished", "requestfailed"]) {
  page.on(done, (r) => { if (new URL(r.url()).pathname.startsWith("/maps/")) inflight = Math.max(0, inflight - 1); });
}
let failed = 0;
try {
  await page.goto(plan.url, { waitUntil: "load", timeout: 60_000 });
  await ready(page);
  for (const view of plan.views) failed += (await capture(page, view)) ? 0 : 1;
} catch (error) {
  for (const view of plan.views) {
    try { readFileSync(`${view.name}.json`); } catch { writeFileSync(`${view.name}.json`, JSON.stringify({ name: view.name, error: String(error.message ?? error), page_errors: [] })); }
  }
  failed = plan.views.length;
}
await context.close();
await browser.close();
process.exit(0);
