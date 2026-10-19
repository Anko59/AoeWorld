// Runs the scripts the harness expands from the storyboard's library steps
// (steps/script.rs): primitive ops on the filmed page, each wait bounded by its
// own timeout. A failure names the step (its number and label) and what it was
// waiting for, so a take that cannot proceed says where and why.
import { FAILED_MS, failingFor, landed, quiet, sample } from "./readiness.mjs";
import { RECONNECTS, describe, fatal, lostLease, serverError, snapshot, transition, wantsReconnect, watch } from "./pagestate.mjs";
import { centredOn, clickable, nearestOf, steady, worldAt } from "./framing.mjs";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// The settle rule of `make compare` and the showcase view wait, shared in
// compare/capture/readiness.mjs: nothing pending, the frame unchanged for the
// window; every chunk still missing having failed for 12 s fails the wait.
const POLL_MS = 250;
const FATAL_POLL_MS = 1000;

async function settle(page, op) {
  const history = [];
  const deadline = Date.now() + op.timeout_ms;
  for (;;) {
    history.push(await sample(page));
    if (history.length > 200) history.shift();
    if (quiet(history)) return;
    if (failingFor(history) >= FAILED_MS) {
      throw new Error(`every terrain chunk still missing (${history.at(-1).failed}) has failed for ${FAILED_MS / 1000} s`);
    }
    if (Date.now() >= deadline) throw Object.assign(new Error("timed out"), { timeout: true });
    await sleep(POLL_MS);
  }
}

// Polls until `read` returns something, or fails as a timeout.
// `detail` (optional) reads what the page showed instead, for the message.
async function until(timeout, read, detail) {
  const deadline = Date.now() + timeout;
  for (;;) {
    const found = await read();
    if (found) return found;
    if (Date.now() >= deadline) {
      throw Object.assign(new Error("timed out"), { timeout: true, detail: detail ? await detail().catch(() => null) : null });
    }
    await sleep(100);
  }
}

const readAttribute = (page, name) =>
  page.evaluate((attribute) => JSON.parse(document.getElementById("scene")?.getAttribute(attribute) ?? "[]"), name);

// Clicks the nth actor of a kind that is on screen and clear of the HUD
// (the village HUD specs, browser/tests/village/interface.play.spec.ts).
async function clickActor(page, op) {
  const actor = await until(op.timeout_ms, async () => {
    const box = await page.locator("#scene").boundingBox();
    const view = page.viewportSize();
    if (!box || !view) return null;
    const visible = (await readAttribute(page, "data-actors")).filter((a) => a.kind === op.actor
      && a.x > 0 && a.x < view.width && a.y > 54 && a.y < view.height - 208);
    const found = visible[op.nth];
    return found ? { x: box.x + found.x, y: box.y + found.y } : null;
  });
  await page.mouse.move(actor.x, actor.y);
  await page.mouse.click(actor.x, actor.y);
}

const cameraOf = (page) => page.evaluate(() => {
  const m = document.getElementById("minimap");
  return { x: Number(m.dataset.cameraX), y: Number(m.dataset.cameraY), zoom: Number(m.dataset.zoom) };
});

// The village specs' `frameResource`: aim the camera at the nearest resident
// node of a kind so it sits in the middle of the canvas, then right-click it
// where the client says it is. The node is not required to land at a computed
// pixel: the client draws it at its terrain height, a few pixels off the flat
// projection, so only "on screen, clear of the HUD, and not moving" is waited
// for (framing.mjs).
async function gather(page, op) {
  const view = page.viewportSize();
  const targets = () => readAttribute(page, "data-resource-targets");
  const seen = async () => `no kind ${op.kind} in data-resource-targets=${JSON.stringify(await targets())} `
    + `selection=${await page.locator("#selection").getAttribute("data-count")}`;
  const nearest = await until(op.timeout_ms, async () => nearestOf(await targets(), op.kind), seen);
  const zoom = 0.35;
  const camera = await cameraOf(page);
  const aim = centredOn(worldAt(camera, view, nearest), zoom);
  await page.evaluate((hash) => { location.hash = hash; }, `x=${aim.x}&y=${aim.y}&zoom=${aim.zoom}`);
  let previous = null;
  const target = await until(op.timeout_ms, async () => {
    const now = await cameraOf(page);
    const node = nearestOf(await targets(), op.kind);
    const framed = Math.abs(now.x - aim.x) <= 0.01 && Math.abs(now.y - aim.y) <= 0.01 && Math.abs(now.zoom - aim.zoom) <= 0.001
      && node && clickable(view, node);
    const settled = framed && steady(previous, node);
    previous = framed ? node : null;
    return settled ? node : null;
  }, async () => `node was at (${nearest.x},${nearest.y}) under camera ${JSON.stringify(camera)}; aimed at ${JSON.stringify(aim)}, `
    + `camera=${JSON.stringify(await cameraOf(page))}, data-resource-targets=${JSON.stringify(await targets())}`);
  const box = await page.locator("#scene").boundingBox();
  await page.mouse.move(box.x + target.x, box.y + target.y);
  await page.mouse.click(box.x + target.x, box.y + target.y, { button: "right" });
}

async function perform(page, op) {
  const timeout = op.timeout_ms;
  switch (op.op) {
    case "click":
      if (op.unless && (await page.$(op.unless)) !== null) return;
      return page.click(op.selector, { timeout });
    case "check":
      return page.check(op.selector, { timeout });
    case "select_option":
      return page.selectOption(op.selector, op.value, { timeout });
    case "click_canvas": {
      const box = await page.locator("#scene").boundingBox({ timeout });
      if (!box) throw new Error("the canvas #scene has no box");
      const x = box.x + box.width / 2 + op.dx;
      const y = box.y + box.height / 2 + op.dy;
      await page.mouse.move(x, y);
      return page.mouse.click(x, y, { button: op.right ? "right" : "left" });
    }
    case "key":
      return page.keyboard.press(op.value);
    case "click_actor":
      return clickActor(page, op);
    case "gather":
      return gather(page, op);
    case "hash":
      return page.evaluate((hash) => { location.hash = hash; }, op.value);
    case "wait":
      return page.waitForSelector(op.selector, { state: "visible", timeout });
    case "wait_attached":
      return page.waitForSelector(op.selector, { state: "attached", timeout });
    case "wait_hidden":
      return page.waitForSelector(op.selector, { state: "hidden", timeout });
    case "wait_camera": {
      const [x, y, zoom] = op.camera;
      const deadline = Date.now() + timeout;
      while (!landed([await sample(page)], { x, y, zoom })) {
        if (Date.now() >= deadline) throw Object.assign(new Error("timed out"), { timeout: true });
        await sleep(100);
      }
      return;
    }
    case "settle":
      return settle(page, op);
    default:
      throw new Error(`unknown op ${op.op}`);
  }
}

// Runs one library step's script; `number` is the step's place in its scene.
const logs = new WeakMap();

// Runs `perform`, and stops it as soon as the page can no longer succeed.
let lastPhase = null;
let observingSince = null;
let errorSince = null;
let reconnects = 0;
const taken = Date.now();
async function guarded(page, op) {
  let stop = false;
  const watcher = (async () => {
    while (!stop) {
      await sleep(FATAL_POLL_MS);
      if (stop) return;
      const state = await snapshot(page).catch(() => ({}));
      const phase = transition(state);
      if (phase !== lastPhase) console.log("showcase-phase page_state", Date.now() - taken, phase);
      lastPhase = phase;
      observingSince = state.observing ? (observingSince ?? Date.now()) : null;
      errorSince = serverError(state) ? (errorSince ?? Date.now()) : null;
      if (wantsReconnect(state, errorSince, Date.now()) && reconnects < RECONNECTS) {
        reconnects++;
        errorSince = null;
        console.log("showcase-phase reconnect", reconnects, state.connection);
        await page.evaluate(() => document.getElementById("reconnect")?.click()).catch(() => {});
      }
      const stuck = reconnects >= RECONNECTS && wantsReconnect(state, errorSince, Date.now())
        ? `the connection still reads "${state.connection}" after ${RECONNECTS} reconnects` : null;
      const reason = fatal(state) ?? stuck ?? lostLease(observingSince, Date.now());
      if (reason) throw Object.assign(new Error(reason), { fatal: true });
    }
  })();
  try {
    return await Promise.race([perform(page, op), watcher]);
  } finally {
    stop = true;
    watcher.catch(() => {});
  }
}

// Where the page was when a step failed: its state and the latest errors.
async function where(page) {
  const state = await snapshot(page).then(describe, (error) => `unreadable: ${error.message.split("\n")[0]}`);
  const errors = logs.get(page) ?? [];
  return ` [page: ${state}${errors.length ? `; errors: ${errors.join(" | ")}` : ""}]`;
}

export async function runScript(page, script, number) {
  const stepStarted = Date.now();
  reconnects = 0;
  if (!logs.has(page)) logs.set(page, watch(page));
  for (const op of script.ops) {
    const began = Date.now();
    try {
      await guarded(page, op);
      // Which wait took the time: a slow start shows up here, not as a lump.
      if (Date.now() - began >= 1000) console.log("showcase-phase op_ms", Date.now() - began, number, op.what);
    } catch (error) {
      const timedOut = error.timeout || error.name === "TimeoutError";
      const detail = String(error.message).split("\n")[0];
      const seen = error.detail ? ` (${error.detail})` : "";
      throw new Error(
        `showcase step ${number} \`${script.label}\`: ${error.fatal
          ? `the page failed after ${Date.now() - began} ms while waiting for ${op.what}: ${detail}`
          : timedOut
            ? `timed out after ${Date.now() - began} ms (limit ${op.timeout_ms} ms) waiting for ${op.what}${seen}`
            : `failed while waiting for ${op.what}: ${detail}`}${await where(page)}`,
      );
    }
  }
  console.log("showcase-phase step_ms", Date.now() - stepStarted, number, script.label);
}
