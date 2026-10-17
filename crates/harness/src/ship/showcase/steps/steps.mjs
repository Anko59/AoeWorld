// Runs the scripts the harness expands from the storyboard's library steps
// (steps/script.rs): primitive ops on the filmed page, each wait bounded by its
// own timeout. A failure names the step (its number and label) and what it was
// waiting for, so a take that cannot proceed says where and why.
import { FAILED_MS, failingFor, landed, quiet, sample } from "./readiness.mjs";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// The settle rule of `make compare` and the showcase view wait, shared in
// compare/capture/readiness.mjs: nothing pending, the frame unchanged for the
// window; every chunk still missing having failed for 12 s fails the wait.
const POLL_MS = 250;

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
async function until(timeout, read) {
  const deadline = Date.now() + timeout;
  for (;;) {
    const found = await read();
    if (found) return found;
    if (Date.now() >= deadline) throw Object.assign(new Error("timed out"), { timeout: true });
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

// The village specs' `frameResource`: aim the camera at the nearest resident
// node of a kind so it sits on screen, then right-click it. The isometric
// projection (64 x 32 px per tile at zoom 1) is the client's.
async function gather(page, op) {
  const view = page.viewportSize();
  const nearest = await until(op.timeout_ms, async () =>
    (await readAttribute(page, "data-resource-targets")).filter((r) => r.kind === op.kind)
      .sort((a, b) => a.distance - b.distance)[0]);
  const zoom = 0.35;
  const camera = await page.evaluate(() => {
    const m = document.getElementById("minimap");
    return { x: Number(m.dataset.cameraX), y: Number(m.dataset.cameraY), zoom: Number(m.dataset.zoom) };
  });
  const dx = (nearest.x - view.width / 2) / (64 * camera.zoom);
  const dy = (nearest.y - view.height / 2) / (32 * camera.zoom);
  const worldX = camera.x + (dx + dy) / 2;
  const worldY = camera.y + (dy - dx) / 2;
  const x = (camera.x + worldX) / 2 + 2;
  const y = (camera.y + worldY) / 2 + 2;
  await page.evaluate((hash) => { location.hash = hash; }, `x=${x}&y=${y}&zoom=${zoom}`);
  const expectedX = view.width / 2 + (worldX - x - (worldY - y)) * 64 * zoom;
  const expectedY = view.height / 2 + (worldX - x + (worldY - y)) * 32 * zoom;
  const target = await until(op.timeout_ms, async () =>
    (await readAttribute(page, "data-resource-targets")).find((r) => r.kind === op.kind
      && Math.abs(r.x - expectedX) < 2 && Math.abs(r.y - expectedY) < 2));
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
export async function runScript(page, script, number) {
  const stepStarted = Date.now();
  for (const op of script.ops) {
    const began = Date.now();
    try {
      await perform(page, op);
    } catch (error) {
      const timedOut = error.timeout || error.name === "TimeoutError";
      const detail = String(error.message).split("\n")[0];
      throw new Error(
        `showcase step ${number} \`${script.label}\`: ${timedOut
          ? `timed out after ${Date.now() - began} ms (limit ${op.timeout_ms} ms) waiting for ${op.what}`
          : `failed while waiting for ${op.what}: ${detail}`}`,
      );
    }
  }
  console.log("showcase-phase step_ms", Date.now() - stepStarted, number, script.label);
}
