/* global window, document, navigator */
import { chromium } from "@playwright/test";
import fs from "node:fs/promises";
import { availableParallelism, loadavg } from "node:os";
import path from "node:path";
import process from "node:process";
import { URL } from "node:url";
import { capture, reportScene, startTrace } from "./stutter_capture.mjs";
import { selectCrowd } from "./stutter_metrics.mjs";

const [inputPath, outputPath] = process.argv.slice(2);
const {
  url,
  client_package_directory: pkg,
  admin_token,
  ...input
} = JSON.parse(await fs.readFile(inputPath, "utf8"));
const headers = { "x-aoeworld-admin-token": admin_token };
const viewport = { width: 1280, height: 720 };
const zoom = 0.102;
const report = { ...input, viewport, zoom, scenes: {} };
const saveReport = () => fs.writeFile(outputPath, JSON.stringify(report));
const browser = await chromium.launch({
  headless: true,
  args: [
    "--no-sandbox",
    "--use-angle=swiftshader",
    "--enable-unsafe-swiftshader",
  ],
});
try {
  const context = await browser.newContext({
    viewport,
    deviceScaleFactor: 1,
    recordVideo: { dir: outputPath + ".video", size: viewport },
  });
  const videoStart = process.hrtime.bigint();
  const page = await context.newPage();
  report.chromium = browser.version();
  report.logical_cpus = availableParallelism();
  const errors = [];
  let networkChanges = 0;
  page.on("pageerror", (error) => {
    errors.push(error.stack ?? String(error));
    process.stderr.write(`${errors.at(-1)}\n`);
  });
  page.on("console", (message) => {
    if (message.type() !== "error") return;
    if (message.text().includes("ERR_NETWORK_CHANGED")) networkChanges++;
    process.stderr.write(`${message.text()} ${message.location().url ?? ""}\n`);
  });
  page.on("requestfailed", (request) => {
    const failure = request.failure()?.errorText;
    if (failure === "net::ERR_NETWORK_CHANGED") networkChanges++;
    if (failure !== "net::ERR_ABORTED") {
      process.stderr.write(`${failure} ${new URL(request.url()).pathname}\n`);
    }
  });
  await page.addInitScript(() => {
    Error.stackTraceLimit = 80;
    Object.defineProperty(navigator, "gpu", { value: undefined });
  });
  await page.route("**/pkg/**", async (route) => {
    const suffix = new URL(route.request().url()).pathname.slice(
      "/pkg/".length,
    );
    if (suffix === "content.json" || suffix.endsWith(".wgsl"))
      return route.continue();
    const file = path.resolve(pkg, suffix);
    if (!file.startsWith(pkg + path.sep))
      throw new Error("invalid package URL");
    await route.fulfill({
      path: file,
      contentType: file.endsWith(".wasm")
        ? "application/wasm"
        : "text/javascript",
    });
  });

  async function request(endpoint, options = {}) {
    const response = await page.request.fetch(url + endpoint, {
      timeout: 120000,
      ...options,
    });
    if (!response.ok())
      throw new Error(
        `${endpoint}: ${response.status()} ${await response.text()}`,
      );
    return response.json();
  }
  await page.goto(url + "/?play=1#renderer=webgl2");
  await page.waitForFunction(
    () =>
      window.localStorage.getItem("aoeworld.resume-token") ||
      document.querySelector("#connection").textContent === "Unavailable",
    null,
    { timeout: 120000 },
  );
  const { token, unavailable } = await page.evaluate(() => ({
    token: window.localStorage.getItem("aoeworld.resume-token"),
    unavailable: document.querySelector("#unsupported").textContent,
  }));
  if (!token) throw new Error(unavailable);
  const started = await request("/skirmish", {
    method: "POST",
    data: { save: input.save.id },
    headers: { "x-aoeworld-controller-token": token },
  });
  for (let attempts = 0; ; attempts++) {
    const status = await request("/skirmish");
    if (status.id !== started.id)
      throw new Error("benchmark match was replaced");
    if (status.phase === "ready") break;
    if (
      ["failed", "cancelled", "stopped"].includes(status.phase) ||
      attempts >= 600
    ) {
      throw new Error(`save restore did not finish: ${JSON.stringify(status)}`);
    }
    await page.waitForTimeout(500);
  }
  process.stdout.write("Saved match restored\n");
  await request("/admin/fog", {
    headers,
    method: "POST",
    data: { disabled: true },
  });
  const match = await request("/admin/match", { headers });
  const entities = [];
  for (const { id, status } of match.players) {
    if (status === "eliminated") continue;
    let offset = 0;
    do {
      const row = await request(
        `/admin/match/players/${id}?offset=${offset}&limit=500`,
        { headers },
      );
      if (row.tick !== match.tick)
        throw new Error("saved scene changed during census");
      entities.push(...row.entities);
      offset = row.next_offset;
    } while (offset !== null);
  }
  report.restored_tick = match.tick;
  report.restored_entities = entities.length;
  report.fog_disabled = match.fog_disabled;
  const backing = await page
    .locator("#scene")
    .evaluate((canvas) => [canvas.width, canvas.height]);
  report.backing = backing;
  const town = selectCrowd(entities, [
    backing[0] / (128 * zoom),
    backing[1] / (64 * zoom),
  ]);
  report.crowd = town;
  const pan = [
    [0.15, 0.25],
    [0.85, 0.75],
  ].map((point) => point.map((n) => n * input.side));
  report.pan = pan;

  async function warmScene(name, center) {
    const [x, y] = center;
    // Each case gets a fresh client at its starting pose. This also avoids
    // dragging the pan's in-flight requests and native cache into the hold.
    await page.goto(
      `${url}/?play=1&scene=${name}#renderer=webgl2&x=${x}&y=${y}&zoom=${zoom}`,
    );
    await page.waitForSelector(
      '#playground[data-assets="aoe2-local"][data-renderer="webgl2"]',
      { timeout: 120000 },
    );
    await page.evaluate(async () => {
      window.aoeNavigation.strategic.skipReveal();
      window.__stutterClient = await import("/pkg/aoe_client.js");
    });
    await page.mouse.move(viewport.width / 2, viewport.height / 2);
    await page.waitForFunction(
      () => {
        const { connected, frame, chunksPending, chunksFailed, viewPainted } =
          document.querySelector("#scene").dataset;
        return (
          connected === "true" &&
          Number(frame) > 0 &&
          chunksPending === "0" &&
          chunksFailed === "0" &&
          viewPainted === "true"
        );
      },
      null,
      { timeout: 120000 },
    );
  }

  async function scene(name, center, path) {
    const changesBeforeWarmup = networkChanges;
    try {
      await warmScene(name, center);
    } catch (error) {
      if (networkChanges === changesBeforeWarmup) throw error;
      process.stderr.write(
        `Retrying ${name} warmup after Chromium network change\n`,
      );
      await warmScene(name, center);
    }
    report.renderer = await page.getAttribute("#playground", "data-renderer");
    report.webgl_device = await page.locator("#scene").evaluate((canvas) => {
      const gl = canvas.getContext("webgl2");
      const debug = gl.getExtension("WEBGL_debug_renderer_info");
      return debug
        ? gl.getParameter(debug.UNMASKED_RENDERER_WEBGL)
        : gl.getParameter(gl.RENDERER);
    });
    const visible = await page.evaluate(() =>
      Array.from(window.__stutterClient.frame_bench_scene_counts()),
    );
    if (
      name === "crowded_hold" &&
      (visible[0] < 1000 || visible[1] === 0 || visible[2] === 0)
    ) {
      throw new Error(
        `crowded viewport lacks units, buildings, or attackers: ${visible}`,
      );
    }
    process.stdout.write(
      `${name}: start; rendered units/buildings/attackers ${visible.join("/")}\n`,
    );
    const stopTrace = await startTrace(page);
    const video_offset_ms = Number(process.hrtime.bigint() - videoStart) / 1e6;
    const load_at_start = loadavg();
    const raw = await page.evaluate(capture, {
      name,
      seconds: input.scene_seconds,
      pan: path,
      zoom,
    });
    const trace = await stopTrace(outputPath + `.${name}.trace.json`);
    report.scenes[name] = {
      visible,
      load_at_start,
      video_offset_ms,
      ...reportScene(name, raw, trace),
    };
    await saveReport();
  }

  await scene("crowded_hold", [town.x, town.y], null);
  await scene("fast_pan", pan[0], pan);
  if (errors.length) throw new Error(errors.join("\n"));
  const video = page.video();
  await context.close();
  report.video = await video.path();
  await saveReport();
} finally {
  await browser.close();
}
