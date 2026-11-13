/* global window, navigator, document, performance, setTimeout */
import { chromium } from "@playwright/test";
import { Buffer } from "node:buffer";
import fs from "node:fs/promises";
import path from "node:path";
import crypto from "node:crypto";
import process from "node:process";
import { URL } from "node:url";
import { installCounters, names } from "./counters.mjs";

const [inputPath, outputPath, recording] = process.argv.slice(2);
const input = JSON.parse(await fs.readFile(inputPath, "utf8"));
const privatePkg = input.client_package_directory;
const sha = (value) => crypto.createHash("sha256").update(value).digest("hex");
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
    viewport: { width: 1280, height: 720 },
    deviceScaleFactor: 1,
    ...(recording === "record"
      ? {
          recordVideo: {
            dir: path.join(path.dirname(outputPath), "video"),
            size: { width: 1280, height: 720 },
          },
        }
      : {}),
  });
  const page = await context.newPage();
  page.setDefaultTimeout(120000);
  const errors = [];
  page.on("pageerror", (error) => errors.push(String(error)));
  page.on("console", (message) => {
    if (message.type() === "error") process.stderr.write(`${message.text()}\n`);
  });
  await page.route("**/pkg/**", async (route) => {
    const suffix = new URL(route.request().url()).pathname.slice(
      "/pkg/".length,
    );
    if (suffix === "content.json" || suffix.endsWith(".wgsl"))
      return route.continue();
    const file = path.resolve(privatePkg, suffix);
    if (!file.startsWith(privatePkg + path.sep))
      throw new Error("invalid package URL");
    await route.fulfill({
      path: file,
      contentType: file.endsWith(".wasm")
        ? "application/wasm"
        : "text/javascript",
    });
  });
  await page.addInitScript({
    content: `window.__frameCost = (${installCounters.toString()})(WebGL2RenderingContext.prototype);`,
  });
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "gpu", { value: undefined });
    const request = window.requestAnimationFrame.bind(window);
    const pending = new Map();
    let id = 0,
      paused = false;
    window.requestAnimationFrame = (callback) => {
      const key = ++id;
      pending.set(key, callback);
      if (!paused)
        request((time) => {
          if (!paused && pending.delete(key)) callback(time);
        });
      return key;
    };
    window.cancelAnimationFrame = (key) => pending.delete(key);
    window.__frameClock = {
      pause() {
        paused = true;
      },
      step(time) {
        const callbacks = [...pending.values()];
        pending.clear();
        for (const callback of callbacks) callback(time);
      },
    };
  });
  await page.goto(input.url + "/?play=1#renderer=webgl2");
  await page.waitForFunction(
    () =>
      document.querySelector("#playground")?.dataset.assets === "aoe2-local",
  );
  await page.evaluate(async (side) => {
    window.__frameClock.pause();
    window.__bench = await import("/pkg/aoe_client.js");
    window.__bench.frame_bench_freeze(side);
  }, input.side);
  const bridgeUrl = await page.evaluate(
    () =>
      performance
        .getEntriesByType("resource")
        .find((e) => e.name.endsWith("/src/game_renderer/webgl/bridge.js"))
        ?.name,
  );
  if (!bridgeUrl) throw new Error("WebGL bridge not served");
  await page.evaluate(async (url) => {
    const { AoeWebGl } = await import(url);
    const render = AoeWebGl.prototype.render;
    AoeWebGl.prototype.render = function (...args) {
      window.__frameGl = this;
      return render.apply(this, args);
    };
  }, bridgeUrl);
  async function drain() {
    await page.evaluate(async () => {
      const bridge = window.__frameGl;
      if (!bridge) return;
      bridge.gl.finish();
      const deadline = performance.now() + 10000;
      // WebGL sync status becomes observable only across event-loop turns.
      while (bridge.pendingFrames.length) {
        await new Promise((resolve) => setTimeout(resolve, 0));
        bridge.poll();
        if (performance.now() > deadline)
          throw new Error("GPU completion timed out");
      }
    });
  }
  const chunkCoordinates = new Set();
  const bound = Math.ceil(input.side / 32);
  for (const [x, y] of input.script) {
    for (
      let cy = Math.max(0, Math.floor(y / 32) - 5);
      cy <= Math.min(bound - 1, Math.floor(y / 32) + 5);
      cy++
    )
      for (
        let cx = Math.max(0, Math.floor(x / 32) - 5);
        cx <= Math.min(bound - 1, Math.floor(x / 32) + 5);
        cx++
      )
        chunkCoordinates.add(`${cx}/${cy}`);
  }
  const chunks = [];
  for (const coordinate of [...chunkCoordinates].sort()) {
    const response = await page.request.get(
      `${input.url}/maps/${input.content_hash}/chunks/${coordinate}`,
    );
    if (!response.ok())
      throw new Error(`chunk ${coordinate}: ${response.status()}`);
    chunks.push(await response.json());
  }
  await page.evaluate(
    ({ hash, chunks }) =>
      window.__bench.activate_surface_fixture(hash, JSON.stringify(chunks)),
    { hash: input.content_hash, chunks },
  );
  // Native residency has a bounded LRU. Settle one view, then clear only its
  // CPU scene cache: the first counted frame includes the camera rebuild.
  async function warm(view) {
    await page.evaluate(
      (view) => window.__bench.frame_bench_camera(...view, true),
      view,
    );
    let ready = false,
      stable = 0,
      painted = false;
    for (let attempt = 0; attempt < 600; attempt++) {
      const state = await page.evaluate(() => {
        const canvas = document.querySelector("#scene");
        const before = Number(canvas.dataset.frame ?? "0");
        window.__frameCost.reset();
        window.__frameClock.step(1000);
        const costs = window.__frameCost.take();
        canvas.getContext("webgl2").finish();
        return {
          painted: Number(canvas.dataset.frame) > before,
          settled: window.__bench.frame_bench_settled() && costs[1] === 0,
        };
      });
      painted ||= state.painted;
      await drain();
      stable = state.settled ? stable + 1 : 0;
      ready = painted && stable >= 5;
      if (ready) break;
      await page.waitForTimeout(20);
    }
    if (!ready) throw new Error("native scene failed to warm");
    await page.evaluate(() => {
      window.__bench.frame_bench_reset_clock();
      window.__bench.frame_bench_reset_scene();
    });
  }
  const frames = [];
  for (let frame = 0; frame < input.script.length; frame++) {
    if (frame % 8 === 0) await warm(input.script[frame]);
    const row = await page.evaluate(
      ({ view, frame }) => {
        const api = window.__bench,
          canvas = document.querySelector("#scene");
        api.frame_bench_camera(...view, true);
        api.frame_bench_take();
        window.__frameCost.reset();
        const before = Number(canvas.dataset.frame);
        // Freeze animation as well as authority: warming and sampling must ask
        // for the same native pose, regardless of browser/host scheduling.
        window.__frameClock.step(1000);
        const costs = Array.from(api.frame_bench_take(), Number);
        const submitted = window.__frameCost.take();
        canvas.getContext("webgl2").finish();
        if (Number(canvas.dataset.frame) !== before + 1)
          throw new Error(
            `script frame ${frame} was not rendered: ${JSON.stringify({ view, scene: { ...canvas.dataset }, costs, submitted })}`,
          );
        if (
          costs[5] !== Number(frame % 8 === 0) ||
          submitted[5] === 0 ||
          submitted[6] === 0
        )
          throw new Error(
            `frame ${frame} did not execute the expected scene/draw work`,
          );
        return [...costs, ...submitted];
      },
      { view: input.script[frame], frame },
    );
    await drain();
    const probe = await page.evaluate(() => {
      window.__frameCost.reset();
      window.__bench.frame_bench_upload_probe();
      return window.__frameCost.take();
    });
    row.push(probe[0], probe[1], probe[2]);
    if (row.some((value) => !Number.isSafeInteger(value) || value < 0))
      throw new Error("invalid integer counter");
    frames.push(row);
    if (recording === "record" && [0, 24, 47].includes(frame))
      await page.screenshot({ path: `${outputPath}.frame-${frame}.png` });
  }
  const costNames = await page.evaluate(() =>
    window.__bench.frame_bench_names().split(","),
  );
  const bridge = await page.evaluate(
    async (url) =>
      Array.from(
        new Uint8Array(await (await globalThis.fetch(url)).arrayBuffer()),
      ),
    bridgeUrl,
  );
  if (errors.length) throw new Error(errors.join("\n"));
  const video = page.video();
  await context.close();
  await fs.writeFile(
    outputPath,
    JSON.stringify({
      counters: [
        ...costNames,
        ...names,
        "upload_probe.error_checks",
        "upload_probe.calls",
        "upload_probe.bytes",
      ],
      frames,
      wasm_sha256: sha(
        await fs.readFile(path.join(privatePkg, "aoe_client_bg.wasm")),
      ),
      bridge_sha256: sha(Buffer.from(bridge)),
      chunks_sha256: sha(JSON.stringify(chunks)),
      video: video ? await video.path() : null,
    }),
  );
} finally {
  await browser.close();
}
