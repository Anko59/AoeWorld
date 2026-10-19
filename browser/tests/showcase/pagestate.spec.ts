import { copyFileSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { expect } from "@playwright/test";
import { test } from "../../server.js";

// The recorder runs these modules side by side in its work directory
// (workdir.rs), readiness.mjs among them: lay out the same directory.
const root = new URL("../../../crates/harness/src/ship/", import.meta.url);
const work = mkdtempSync(join(tmpdir(), "showcase-steps-"));
for (const [name, from] of [
  ["steps.mjs", "showcase/steps/steps.mjs"],
  ["pagestate.mjs", "showcase/steps/pagestate.mjs"],
  ["framing.mjs", "showcase/steps/framing.mjs"],
  ["readiness.mjs", "compare/capture/readiness.mjs"],
]) {
  copyFileSync(new URL(from, root), join(work, name));
}
const { runScript } = await import(pathToFileURL(join(work, "steps.mjs")).href);
const state = await import(pathToFileURL(join(work, "pagestate.mjs")).href);
const framing = await import(pathToFileURL(join(work, "framing.mjs")).href);

const stuck = `<body data-start-state="loading">
<div id="connection">connected</div><p id="unsupported"></p>
<div id="loading-screen"><h1 id="loading-title">Opening your world</h1>
<p id="loading-phase">Opening terrain</p><p id="loading-error" hidden>nope</p></div>
<canvas id="scene" data-chunks-pending="3" data-chunks-failed="0" data-frame="4"></canvas>
<div id="minimap" data-raster="pending" data-map-hash="abc"></div></body>`;

const waitGame = (timeout_ms: number) => ({
  label: 'wait_for {"state":"game"}',
  ops: [
    {
      op: "wait",
      selector: 'body[data-start-state="game"]',
      timeout_ms,
      what: "the match on screen (body data-start-state=game)",
    },
  ],
});

test("a timeout names the state the page was stuck in", async ({ page }) => {
  await page.setContent(stuck);
  const error = await runScript(page, waitGame(300), 2).catch((e: Error) => e);
  expect(error.message).toContain("timed out after");
  expect(error.message).toContain('start_state="loading"');
  expect(error.message).toContain('loading_title="Opening your world"');
  expect(error.message).toContain('raster="pending"');
  expect(error.message).toContain('chunks_pending="3"');
});

test("a client that failed to start fails the step at once", async ({ page }) => {
  await page.setContent(stuck);
  await page.evaluate(() => {
    document.getElementById("unsupported")!.textContent =
      "Unable to start AoeWorld: AoE II assets are unavailable.";
    document.getElementById("connection")!.textContent = "Unavailable";
  });
  const started = Date.now();
  const error = await runScript(page, waitGame(60_000), 1).catch((e: Error) => e);
  expect(Date.now() - started).toBeLessThan(10_000);
  expect(error.message).toContain("the page failed after");
  expect(error.message).toContain("assets are unavailable");
  expect(error.message).not.toContain("timed out");
});

test("a loading screen error fails the step at once", async ({ page }) => {
  await page.setContent(stuck);
  await page.evaluate(() => {
    document.getElementById("loading-error")!.hidden = false;
    document.getElementById("loading-error")!.textContent =
      "We couldn't open your landscape. The terrain could not finish loading.";
  });
  const error = await runScript(page, waitGame(60_000), 1).catch((e: Error) => e);
  expect(error.message).toContain("loading screen shows an error");
  expect(error.message).toContain("terrain could not finish loading");
});

test("a healthy page is not reported as failed", async ({ page }) => {
  await page.setContent(stuck);
  expect(state.fatal(await state.snapshot(page))).toBeNull();
  await page.evaluate(() => {
    document.body.dataset.startState = "game";
  });
  await runScript(page, waitGame(2000), 1);
});

test("console errors of the page are listed with the failure", async ({ page }) => {
  await page.setContent(stuck);
  await runScript(page, { label: "noop", ops: [] }, 1);
  await page.evaluate(() => console.error("boom from the client"));
  const error = await runScript(page, waitGame(300), 3).catch((e: Error) => e);
  expect(error.message).toContain("boom from the client");
});

test("a page that only observes for longer than the lease wait gave up its role", async () => {
  expect(state.lostLease(null, 1_000_000)).toBeNull();
  expect(state.lostLease(1_000_000, 1_000_000 + state.LEASE_WAIT_MS - 1)).toBeNull();
  expect(state.lostLease(1_000_000, 1_000_000 + state.LEASE_WAIT_MS)).toContain(
    "controller lease",
  );
});

test("a server error on the connection label while loading asks for a reconnect", async () => {
  const loading = { start_state: "loading", connection: "server error: invalid gameplay subscription region or revision" };
  expect(state.serverError(loading)).toBe(true);
  expect(state.serverError({ connection: "connected" })).toBe(false);
  expect(state.wantsReconnect(loading, null, 10_000)).toBe(false);
  expect(state.wantsReconnect(loading, 1000, 1000 + state.ERROR_SETTLE_MS - 1)).toBe(false);
  expect(state.wantsReconnect(loading, 1000, 1000 + state.ERROR_SETTLE_MS)).toBe(true);
  expect(state.wantsReconnect({ ...loading, start_state: "game" }, 1000, 99_000)).toBe(false);
});

test("the step presses Reconnect when the label is stuck on a server error", async ({ page }) => {
  await page.setContent(`${stuck}<button id="reconnect">Reconnect</button>`);
  await page.evaluate(() => {
    document.getElementById("connection")!.textContent = "server error: invalid gameplay subscription region or revision";
    document.getElementById("reconnect")!.addEventListener("click", () => {
      document.getElementById("connection")!.textContent = "connected";
      document.body.dataset.startState = "game";
    });
  });
  await runScript(page, waitGame(30_000), 1);
  expect(await page.locator("#connection").textContent()).toBe("connected");
});

test("gather frames a node by what is on screen, not by a computed pixel", async () => {
  const view = { width: 1280, height: 720 };
  const camera = { x: 267, y: 355, zoom: 1 };
  // The node drawn at (704, 168) under that camera: its world tile, then the
  // camera centred on it puts it in the middle, clear of the HUD.
  const world = framing.worldAt(camera, view, { x: 704, y: 168 });
  expect(framing.centredOn(world, 0.35)).toEqual({ x: world.x, y: world.y, zoom: 0.35 });
  expect(framing.clickable(view, { x: 640, y: 360 })).toBe(true);
  expect(framing.clickable(view, { x: 640, y: 40 })).toBe(false);
  expect(framing.clickable(view, { x: 640, y: 700 })).toBe(false);
  // The client drew the node 2.8 px below the flat projection; still accepted.
  expect(framing.steady({ x: 651.2, y: 284.4 }, { x: 651.2, y: 284.4 })).toBe(true);
  expect(framing.steady({ x: 651.2, y: 284.4 }, { x: 655, y: 284.4 })).toBe(false);
  const targets = [
    { kind: 0, x: 651.2, y: 284.4, distance: 2 },
    { kind: 0, x: 10, y: 10, distance: 9 },
    { kind: 1, x: 700, y: 400, distance: 1 },
  ];
  expect(framing.nearestOf(targets, 0)?.distance).toBe(2);
  expect(framing.nearestOf(targets, 3)).toBeUndefined();
});
