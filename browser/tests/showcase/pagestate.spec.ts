import { copyFileSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { expect, type Page } from "@playwright/test";
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
] as const) {
  copyFileSync(new URL(from, root), join(work, name));
}
const { runScript } = await import(pathToFileURL(join(work, "steps.mjs")).href);
const state = await import(pathToFileURL(join(work, "pagestate.mjs")).href);
const framing = await import(pathToFileURL(join(work, "framing.mjs")).href);
const failure = (error: Error) => error;

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
  const error = await runScript(page, waitGame(300), 2).catch(failure);
  expect(error.message).toContain("timed out after");
  expect(error.message).toContain('start_state="loading"');
  expect(error.message).toContain('loading_title="Opening your world"');
  expect(error.message).toContain('raster="pending"');
  expect(error.message).toContain('chunks_pending="3"');
});

test("a failed client fails the step immediately", async ({ page }) => {
  await page.setContent(stuck);
  await page.evaluate(() => {
    document
      .getElementById("unsupported")
      ?.replaceChildren(
        "Unable to start AoeWorld: AoE II assets are unavailable.",
      );
    document.getElementById("connection")?.replaceChildren("Unavailable");
  });
  const started = Date.now();
  const error = await runScript(page, waitGame(60_000), 1).catch(failure);
  expect(Date.now() - started).toBeLessThan(10_000);
  expect(error.message).toContain("the page failed after");
  expect(error.message).toContain("assets are unavailable");
  expect(error.message).not.toContain("timed out");
});

test("a loading screen error fails the step at once", async ({ page }) => {
  await page.setContent(stuck);
  await page.evaluate(() => {
    document.getElementById("loading-error")?.removeAttribute("hidden");
    document
      .getElementById("loading-error")
      ?.replaceChildren(
        "We couldn't open your landscape. The terrain could not finish loading.",
      );
  });
  const error = await runScript(page, waitGame(60_000), 1).catch(failure);
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

test("failures include console errors", async ({ page }) => {
  await page.setContent(stuck);
  await runScript(page, { label: "noop", ops: [] }, 1);
  await page.evaluate(() => console.error("boom from the client"));
  const error = await runScript(page, waitGame(300), 3).catch(failure);
  expect(error.message).toContain("boom from the client");
});

test("loading server errors request a reconnect", async () => {
  const loading = {
    start_state: "loading",
    connection:
      "server error: invalid gameplay subscription region or revision",
  };
  expect(state.serverError(loading)).toBe(true);
  expect(state.serverError({ connection: "connected" })).toBe(false);
  expect(state.wantsReconnect(loading, null, 10_000)).toBe(false);
  const settled = 1000 + state.ERROR_SETTLE_MS;
  expect(state.wantsReconnect(loading, 1000, settled - 1)).toBe(false);
  expect(state.wantsReconnect(loading, 1000, settled)).toBe(true);
  expect(
    state.wantsReconnect({ ...loading, start_state: "game" }, 1000, 99_000),
  ).toBe(false);
});

test("steps reconnect a stuck server error", async ({ page }) => {
  await page.setContent(`${stuck}<button id="reconnect">Reconnect</button>`);
  await page.evaluate(() => {
    document
      .getElementById("connection")
      ?.replaceChildren(
        "server error: invalid gameplay subscription region or revision",
      );
    document.getElementById("reconnect")?.addEventListener("click", () => {
      document.getElementById("connection")?.replaceChildren("connected");
      document.body.dataset.startState = "game";
    });
  });
  await runScript(page, waitGame(30_000), 1);
  expect(await page.locator("#connection").textContent()).toBe("connected");
});

test("gather uses reported screen coordinates", async () => {
  const view = { width: 1280, height: 720 };
  const camera = { x: 267, y: 355, zoom: 1 };
  // The node drawn at (704, 168) under that camera: its world tile, then the
  // camera centred on it puts it in the middle, clear of the HUD.
  const world = framing.worldAt(camera, view, { x: 704, y: 168 });
  const aim = { x: world.x, y: world.y, zoom: 0.35 };
  expect(framing.centredOn(world, 0.35)).toEqual(aim);
  expect(framing.clickable(view, { x: 640, y: 360 })).toBe(true);
  expect(framing.clickable(view, { x: 640, y: 40 })).toBe(false);
  expect(framing.clickable(view, { x: 640, y: 700 })).toBe(false);
  // The client drew the node 2.8 px below the flat projection; still accepted.
  const node = { x: 651.2, y: 284.4 };
  expect(framing.steady(node, node)).toBe(true);
  expect(framing.steady(node, { x: 655, y: 284.4 })).toBe(false);
  const targets = [
    { kind: 0, x: 651.2, y: 284.4, distance: 2 },
    { kind: 0, x: 10, y: 10, distance: 9 },
    { kind: 1, x: 700, y: 400, distance: 1 },
  ];
  expect(framing.nearestOf(targets, 0)?.distance).toBe(2);
  expect(framing.nearestOf(targets, 3)).toBeUndefined();
});

// Only the HUD publisher advances this revision, every 250 ms in the client.
// A camera read can therefore be current while the resource positions are old.
type GatherSetup = { clamp: boolean };

async function gatherPage(page: Page, clamp: boolean) {
  await page.setViewportSize({ width: 1280, height: 720 });
  // Serve the fixture at the game's origin: the current HUD imports its icon
  // module by absolute path, which cannot resolve from a data URL.
  await page.route("**/gather-fixture", (route) =>
    route.fulfill({
      contentType: "text/html",
      body: `<body data-start-state="game" style="margin:0">
    <canvas id="scene" width="1280" height="720" data-hud-revision="0"></canvas>
    <div id="minimap" data-camera-x="100" data-camera-y="100" data-zoom="1"
      data-world-width="1000" data-world-height="1000"></div>
    <div id="resource-highlight"></div><div id="minimap-entities"></div><div id="minimap-resources"></div>
    <div id="selection" data-count="1"></div></body>`,
    }),
  );
  await page.goto("/gather-fixture");
  const setup = async ({ clamp }: GatherSetup) => {
    const moduleUrl = "/interface/details.js";
    const { updateDetails } = await import(moduleUrl);
    const publish = (x: number, y: number) =>
      updateDetails(new Float64Array([1, -1, 0, 0, 0, 0, 0, 0, 0, 0, x, y, 2]));
    publish(704, 300);
    const scene = document.getElementById("scene");
    const map = document.getElementById("minimap");
    if (!scene || !map) throw new Error("missing gather fixture");
    scene.addEventListener("contextmenu", (event) => {
      scene.dataset.clicked = JSON.stringify({
        x: event.clientX,
        y: event.clientY,
      });
      scene.dataset.clickedRevision = scene.dataset.hudRevision;
    });
    window.addEventListener(
      "hashchange",
      () => {
        const aim = new URLSearchParams(location.hash.slice(1));
        map.dataset.cameraX = aim.get("x") ?? "missing";
        map.dataset.cameraY = aim.get("y") ?? "missing";
        map.dataset.zoom = aim.get("zoom") ?? "missing";
        if (clamp) {
          map.dataset.cameraX = "90";
          map.dataset.cameraY = "90";
        }
        // Keep old coordinates long enough for two identical pre-move reads.
        setTimeout(() => {
          publish(650, 350);
        }, 500);
      },
      { once: true },
    );
  };
  await page.evaluate(setup, { clamp });
}

const gatherScript = {
  label: 'command {"action":"gather","resource":"food"}',
  ops: [
    { op: "gather", kind: 0, timeout_ms: 2000, what: "the food node framed" },
  ],
};

test("gather rejects pre-move HUD coordinates", async ({ page }) => {
  await gatherPage(page, false);
  await runScript(page, gatherScript, 1);
  const scene = page.locator("#scene");
  const revision = await scene.getAttribute("data-clicked-revision");
  const clicked = await scene.getAttribute("data-clicked");
  expect(revision).toBe("2");
  expect(JSON.parse(clicked ?? "null")).toEqual({ x: 650, y: 350 });
});

test("gather accepts a stopped clamped camera", async ({ page }) => {
  await gatherPage(page, true);
  await runScript(page, gatherScript, 1);
  const clicked = await page.locator("#scene").getAttribute("data-clicked");
  expect(JSON.parse(clicked ?? "null")).toEqual({ x: 650, y: 350 });
});
