// Shared compare/showcase polling. Rust replays bookmark evidence using
// compare::settle and compare::capture::refusal before publishing a take.
const CAMERA_TILES = 0.01;
const CAMERA_ZOOM = 0.001;
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// One poll of the page, stamped with the time it was taken. A missing
// attribute reads as -1 (never settled).
export async function sample(page) {
  const at = Date.now();
  const read = await page.evaluate(() => {
    const num = (v) => (v == null || v === "" ? -1 : Number(v));
    const scene = document.getElementById("scene");
    const minimap = document.getElementById("minimap");
    const camera = minimap?.dataset.cameraX === undefined ? null : {
      x: Number(minimap.dataset.cameraX), y: Number(minimap.dataset.cameraY), zoom: Number(minimap.dataset.zoom),
    };
    return {
      connected: document.getElementById("connection")?.textContent === "connected",
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
export const FAILED_MS = 12000;
export function windowMs(history) {
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
export function failingFor(history) {
  const failing = (s) => s.failed > 0 && s.failed === s.pending;
  const last = history.at(-1);
  if (!last || !failing(last)) return 0;
  let start = history.length - 1;
  while (start > 0 && failing(history[start - 1])) start--;
  return last.at - history[start].at;
}
// Nothing pending, and the frame unchanged for the window.
export function quiet(history) {
  const last = history.at(-1);
  if (!last || last.pending !== 0 || last.failed !== 0 || !Number.isFinite(last.frame) || last.frame <= 0) return false;
  let start = history.length - 1;
  while (start > 0 && history[start - 1].pending === 0 && history[start - 1].frame === last.frame) start--;
  return last.at - history[start].at >= windowMs(history);
}
// The camera must also be where the fragment put it.
export function landed(history, view) {
  if (!view) return true;
  const c = history.at(-1)?.camera;
  return Boolean(c) && Math.abs(c.x - view.x) <= CAMERA_TILES && Math.abs(c.y - view.y) <= CAMERA_TILES
    && Math.abs(c.zoom / view.zoom - 1) <= CAMERA_ZOOM;
}

// A static play camera needs a successful presentation and a connected app.
// UI-only scenes do not call this; explicit storyboard waits remain screen time.
export async function waitForView(page, view = null) {
  const started = Date.now();
  const history = [];
  for (;;) {
    history.push(await sample(page));
    const last = history.at(-1);
    if (failingFor(history) >= FAILED_MS) throw new Error(`${last.failed} chunks failed to load`);
    if (last.connected && quiet(history) && landed(history, view)) {
      return { history, camera: last.camera, map_hash: last.map_hash,
        renderer: last.renderer, settle_ms: Date.now() - started };
    }
    if (Date.now() - started >= 90_000) {
      throw new Error(`view not settled within 90s (pending ${last.pending}, failed ${last.failed}, frame ${last.frame})`);
    }
    await sleep(250);
  }
}

// Reuse the app document for another camera on the same origin and renderer.
// A backend switch needs a new document: the client reads it only on load.
export function sameCameraDocument(current, target) {
  const before = new URL(current);
  const after = new URL(target);
  const oldCamera = new URLSearchParams(before.hash.slice(1));
  const camera = new URLSearchParams(after.hash.slice(1));
  return before.origin === after.origin && before.pathname === after.pathname
    && before.search === after.search && after.pathname === "/"
    && before.hash !== after.hash
    && ["x", "y", "zoom"].some((key) => camera.has(key))
    && oldCamera.get("renderer") === camera.get("renderer");
}

export async function navigateScene(page, scene, previous) {
  if (previous?.kind === "browser" && !(previous.steps?.length)
      && sameCameraDocument(page.url(), scene.url)) {
    await page.evaluate((fragment) => { location.hash = fragment; }, new URL(scene.url).hash);
    return "reuse";
  }
  // goto with a different fragment keeps the document. Leave it first so
  // backend initialization and bookmark camera restoration always run.
  if (page.url() !== "about:blank") await page.goto("about:blank");
  await page.goto(scene.url, { waitUntil: "load", timeout: 30_000 });
  return "reload";
}
