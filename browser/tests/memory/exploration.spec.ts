import {
  launchMemoryBrowser,
  INSPECTOR_HISTORY_BYTES,
  INSPECTOR_RESOURCE_BYTES,
} from "./chromium.js";
import { expect, firefox, type Browser, type Page } from "@playwright/test";
import { writeFile } from "node:fs/promises";
import { test } from "../../server.js";
import {
  explorationFixture,
  memorySnapshot,
  type MemorySnapshot,
} from "./exploration-fixture.js";
import {
  firefoxTabMemory,
  processMemory,
  processTreeMemory,
  type ProcessMemory,
} from "./process.js";

test.use({
  screenshot: "off",
  trace: "off",
  viewport: { width: 1280, height: 720 },
});

async function sweep(page: Page, browser: Browser, firefoxPid?: number) {
  const visits = await explorationFixture(page);
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/?play=1#x=256&y=256&zoom=0.5");
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-assets",
    "aoe2-local",
  );
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-raster",
    "ready",
  );
  const initialHistoryEntries = await page.evaluate(() => history.length);
  const session =
    firefoxPid === undefined ? await browser.newBrowserCDPSession() : null;
  // Keep inspector lifetime fixed throughout the measured workload.
  const target = session ? await page.context().newCDPSession(page) : null;
  await target?.send("Performance.enable");
  const samples: {
    cycle: number;
    memory: MemorySnapshot;
    process: ProcessMemory;
    beforeCollection: ProcessMemory;
    browserProcessTree?: ProcessMemory;
  }[] = [];
  const route: [number, number][] = [];
  for (let row = 0; row < 12; row++)
    for (let column = 0; column < 12; column++)
      route.push([256 + column * 1400, 256 + row * 1400]);
  for (let row = 0; row < 4; row++)
    for (let column = 0; column < 8; column++)
      route.push([956 + column * 1400, 956 + row * 1400]);
  let routeLength = route.length;
  try {
    for (let cycle = 0; cycle < 3; cycle++) {
      // Whole-extent grid plus bounded midpoints until more than twice the
      // resident capacity has been visited. Repeat exactly that route thereafter.
      for (const [index, [x, y]] of route.entries()) {
        if (index >= routeLength) break;
        if (cycle === 0 && index >= 144 && visits.size > 1024) {
          routeLength = index;
          break;
        }
        await page.evaluate(
          ([x, y]) => {
            // Real edge-pan/minimap exploration adds no session-history entries.
            history.replaceState(null, "", `#x=${x}&y=${y}&zoom=0.5`);
            dispatchEvent(new HashChangeEvent("hashchange"));
          },
          [x, y],
        );
        await expect(page.locator("#minimap")).toHaveAttribute(
          "data-camera-x",
          String(x),
        );
        await expect(page.locator("#scene")).toHaveAttribute(
          "data-chunks-pending",
          "0",
        );
        await expect(page.locator("#scene")).toHaveAttribute(
          "data-chunks-failed",
          "0",
        );
      }
      const beforeCollection = session
        ? await processMemory(session)
        : firefoxTabMemory(await processTreeMemory(firefoxPid ?? 0));
      // Compare retained memory at the same collection boundary in both engines.
      // Otherwise uncollected fetch/IPC temporaries and engine GC scheduling
      // make RSS grow despite flat WASM, JS heap and renderer resource counts.
      // Keep pre-GC RSS as evidence (and under Chromium's absolute ceiling).
      await page.requestGC();
      const memory = await memorySnapshot(page);
      if (target) {
        const { metrics } = (await target.send("Performance.getMetrics")) as {
          metrics: { name: string; value: number }[];
        };
        memory.jsHeapBytes =
          metrics.find((metric) => metric.name === "JSHeapUsedSize")?.value ??
          null;
        memory.chromiumDiagnostics = {
          heap: await target.send("Runtime.getHeapUsage"),
          native: await target.send("Memory.getDOMCounters"),
        };
      }
      let process: ProcessMemory;
      let browserProcessTree: ProcessMemory | undefined;
      if (session) {
        process = await processMemory(session);
      } else {
        browserProcessTree = await processTreeMemory(firefoxPid ?? 0);
        process = firefoxTabMemory(browserProcessTree);
      }
      samples.push({
        cycle,
        memory,
        process,
        beforeCollection,
        browserProcessTree,
      });
    }
  } finally {
    await target?.detach();
    await session?.detach();
  }
  return {
    samples,
    waypoints: routeLength,
    initialHistoryEntries,
    uniqueChunks: visits.size,
    refetchedChunks: [...visits.values()].filter((count) => count > 1).length,
    errors,
  };
}

function assertBounded(result: Awaited<ReturnType<typeof sweep>>) {
  expect(result.errors).toEqual([]);
  expect(result.uniqueChunks).toBeGreaterThan(1024);
  expect(result.refetchedChunks).toBeGreaterThan(512);
  for (const sample of result.samples) {
    expect(sample.memory.historyEntries).toBe(result.initialHistoryEntries);
    expect(sample.memory.minimapRgbaBytes).toBe(512 ** 2 * 4);
    if (sample.memory.fogCanvasBytes !== null)
      expect(sample.memory.fogCanvasBytes).toBe(256 ** 2 * 4);
    if (sample.memory.fogWorkingBytes !== null)
      expect(sample.memory.fogWorkingBytes).toBe(2 * 256 ** 2 * 4);
    expect(sample.memory.owned.terrain_chunks).toBeLessThanOrEqual(512);
    expect(sample.memory.owned.terrain_bytes).toBeLessThanOrEqual(
      128 * 1024 ** 2,
    );
    expect(sample.memory.owned.terrain_requests).toBe(0);
    expect(sample.memory.owned.height_probes).toBeLessThanOrEqual(65536);
    expect(sample.memory.owned.height_candidates).toBeLessThanOrEqual(65536);
    expect(sample.memory.owned.fog_bytes).toBeLessThanOrEqual(512 * 1024);
    expect(sample.memory.owned.scene_bytes).toBeLessThanOrEqual(
      128 * 1024 ** 2,
    );
    expect(sample.memory.owned.render_frame_bytes).toBeLessThanOrEqual(
      256 * 1024 ** 2,
    );
    expect(sample.memory.owned.units).toBeLessThanOrEqual(16384);
    expect(sample.memory.owned.unit_bytes).toBeLessThanOrEqual(4 * 1024 ** 2);
    expect(sample.memory.owned.interpolation_bytes).toBeLessThanOrEqual(
      16 * 1024 ** 2,
    );
    expect(sample.memory.owned.buildings_and_ghosts).toBeLessThanOrEqual(16384);
    expect(sample.memory.owned.animals).toBeLessThanOrEqual(4096);
    expect(sample.memory.owned.projectiles).toBeLessThanOrEqual(4096);
    expect(sample.memory.owned.resource_entries).toBeLessThanOrEqual(65536);
    expect(sample.memory.owned.resource_bytes).toBeLessThanOrEqual(1024 ** 2);
    expect(sample.memory.owned.stump_effects).toBeLessThanOrEqual(4096);
    expect(sample.memory.owned.outgoing_message_bytes).toBe(0);
    // Chromium retains its conservative whole-browser ceiling. Firefox's
    // parent grows with Playwright/network instrumentation, even when the game
    // content process and GL objects plateau. Keep that tree as diagnostics,
    // but assert the sole game tab's RSS plateau below, without a browser cap.
    if (!sample.browserProcessTree) {
      expect(sample.process.residentBytes).toBeLessThan(1024 ** 3);
      expect(sample.beforeCollection.residentBytes).toBeLessThan(1024 ** 3);
    }
    for (const [name, bytes] of sample.memory.owned.renderer) {
      if (
        name === "gpu_texture_bytes" &&
        sample.memory.owned.backend === "webgpu"
      ) {
        // Measured 138,825,728 B = the unchanged eight-page RGBA atlas
        // (134,217,728 B) + 1280*720*5. Depth24Plus is budgeted at 4 B/pixel;
        // the shared occlusion/hover R8Uint mask is already viewport-sized
        // and uses the smallest renderable integer format (1 B/pixel).
        // Pin the required targets exactly, rather than enlarging every budget.
        expect(sample.memory.backingPixels).toBe(1280 * 720);
        expect(bytes, name).toBe(138_825_728);
      } else {
        expect(bytes, name).toBeLessThanOrEqual(128 * 1024 ** 2);
      }
    }
  }
  const wasm = result.samples.map((sample) => sample.memory.wasmBytes);
  const heap = result.samples.flatMap((sample) =>
    sample.memory.jsHeapBytes === null ? [] : [sample.memory.jsHeapBytes],
  );
  if (heap.length) {
    expect(heap.length).toBe(3);
    expect(Math.max(...heap) - Math.min(...heap)).toBeLessThanOrEqual(
      64 * 1024 ** 2,
    );
  }
  const rss = result.samples.map((sample) => sample.process.residentBytes);
  // A restarted or substituted content process must not hide retained memory.
  if (result.samples[0]?.browserProcessTree)
    expect(
      result.samples.map((sample) =>
        sample.process.processes.map((p) => p.type),
      ),
    ).toEqual(
      Array(3).fill(result.samples[0].process.processes.map((p) => p.type)),
    );
  expect(Math.max(...wasm) - Math.min(...wasm)).toBeLessThanOrEqual(
    16 * 1024 ** 2,
  );
  expect(Math.max(...rss) - Math.min(...rss)).toBeLessThanOrEqual(
    64 * 1024 ** 2,
  );
}

test("three whole-extent camera sweeps plateau in Chromium", async ({
  playwright,
  launchOptions,
  baseURL,
}, testInfo) => {
  test.setTimeout(300_000);
  // Chromium's default inspector response history holds up to 200 MB beyond
  // the page's JS/WASM heaps. It grows with these real chunk fetches even after
  // GC, so it is not a valid measure of application retention. Keep the same
  // headless shell, project GPU flags, full workload and whole-browser limits.
  const owner = await launchMemoryBrowser(
    playwright.chromium,
    launchOptions.args ?? [],
  );
  try {
    const page = await owner.browser.newPage({
      baseURL,
      viewport: { width: 1280, height: 720 },
    });
    const result = await sweep(page, owner.browser);
    await writeFile(
      testInfo.outputPath("exploration-memory.json"),
      JSON.stringify(
        {
          ...result,
          inspectorHistoryBytes: INSPECTOR_HISTORY_BYTES,
          inspectorResourceBytes: INSPECTOR_RESOURCE_BYTES,
        },
        null,
        2,
      ),
    );
    expect(result.samples).toHaveLength(3);
    for (const sample of result.samples)
      expect(sample.memory.owned.backend).toBe("webgpu");
    assertBounded(result);
  } finally {
    await owner.close();
  }
});

test("three whole-extent camera sweeps plateau in Playwright Firefox", async ({
  baseURL,
}, testInfo) => {
  test.setTimeout(300_000);
  const server = await firefox.launchServer();
  try {
    const browser = await firefox.connect(server.wsEndpoint());
    try {
      const pid = server.process().pid;
      if (pid === undefined) throw new Error("Firefox root PID is unavailable");
      const page = await browser.newPage({
        baseURL,
        viewport: { width: 1280, height: 720 },
      });
      const result = await sweep(page, browser, pid);
      await writeFile(
        testInfo.outputPath("firefox-exploration-memory.json"),
        JSON.stringify(
          {
            ...result,
            limitation:
              "RSS assertions cover the sole game tab's Web Content process; the full browser tree is diagnostic only because the Playwright-instrumented parent grows independently. Firefox exposes no JS heap metric through Playwright; null means unavailable. GPU bytes are owned resource payload, not driver residency. Synthetic terrain and transport, no 99-AI or GTX qualification.",
          },
          null,
          2,
        ),
      );
      assertBounded(result);
    } finally {
      await browser.close();
    }
  } finally {
    await server.close();
  }
});
