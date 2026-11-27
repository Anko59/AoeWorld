import { expect, type Page } from "@playwright/test";
import { test } from "../../../server.js";
import { inputFixture, selectWorker } from "./fixture.js";

type Measurements = {
  enabled: boolean;
  frameMs: number;
  cpuMs: { decode: number; upload: number; draw: number; ui: number };
  totalsMs: { decode: number; upload: number; draw: number; ui: number };
  gpuMs: number | null;
  gpuStatus: string;
};

async function metrics(page: Page) {
  return page.evaluate(
    () => (window as Window & { aoePerformance?: Measurements }).aoePerformance,
  );
}

test("performance URL flag reports frame and section timings and F3 hides it", async ({
  page,
}, info) => {
  const renderer = info.project.name === "webgpu" ? "webgpu" : "webgl2";
  await inputFixture(page, renderer, [], false, 0, undefined, {
    query: "&perf=1",
  });
  const overlay = page.locator("#performance-overlay");
  await expect(overlay).toBeVisible();
  await expect.poll(async () => (await metrics(page))?.enabled).toBe(true);
  await expect
    .poll(async () => (await metrics(page))?.frameMs ?? 0)
    .toBeGreaterThan(0);
  // Firefox clamps performance.now() to 1 ms, so one sub-millisecond draw
  // reads 0; the running total still registers draws that cross a tick.
  await expect
    .poll(async () => (await metrics(page))?.totalsMs.draw ?? 0)
    .toBeGreaterThan(0);
  const sample = await metrics(page);
  expect(sample).toBeDefined();
  if (!sample) throw new Error("Performance hook missing");
  expect(Number.isFinite(sample.frameMs)).toBe(true);
  for (const section of ["decode", "upload", "draw", "ui"] as const) {
    expect(Number.isFinite(sample.cpuMs[section]), section).toBe(true);
    expect(sample.cpuMs[section], section).toBeGreaterThanOrEqual(0);
  }
  expect(
    sample.gpuMs === null ||
      (Number.isFinite(sample.gpuMs) && sample.gpuMs >= 0),
  ).toBe(true);
  const gpuTimerAvailable = await page.locator("#scene").evaluate((canvas) => {
    if (document.getElementById("playground")?.dataset.renderer !== "webgl2")
      return false;
    // This retrieves the renderer's existing context, without a second canvas.
    const gl = (canvas as HTMLCanvasElement).getContext("webgl2");
    return Boolean(
      gl &&
      !gl.isContextLost() &&
      gl.getExtension("EXT_disjoint_timer_query_webgl2") &&
      !gl.getParameter(0x8fbb), // GPU_DISJOINT_EXT from the timer extension.
    );
  });
  if (gpuTimerAvailable) {
    await expect
      .poll(async () => {
        const measurement = await metrics(page);
        if (!measurement) return false;
        return (
          (measurement.gpuMs !== null &&
            Number.isFinite(measurement.gpuMs) &&
            measurement.gpuMs >= 0) ||
          ["GPU context lost", "GPU clock disjoint"].includes(
            measurement.gpuStatus,
          )
        );
      })
      .toBe(true);
  }
  await expect(overlay).toContainText(/frame/i);
  await expect(overlay).toContainText(/decode/i);
  await expect(overlay).toContainText(/upload/i);
  await expect(overlay).toContainText(/draw/i);
  await expect(overlay).toContainText(/ui/i);
  await page.keyboard.press("F3");
  await expect(overlay).toBeHidden();
  await expect.poll(async () => (await metrics(page))?.enabled).toBe(false);
  await page.keyboard.press("F3");
  await expect(overlay).toBeVisible();
  await expect.poll(async () => (await metrics(page))?.enabled).toBe(true);
});

test("performance sampling starts disabled and F3 enables it", async ({
  page,
}, info) => {
  const renderer = info.project.name === "webgpu" ? "webgpu" : "webgl2";
  await inputFixture(page, renderer);
  await expect(page.locator("#performance-overlay")).toBeHidden();
  await expect.poll(async () => (await metrics(page))?.enabled).toBe(false);
  await page.keyboard.press("F3");
  await expect(page.locator("#performance-overlay")).toBeVisible();
  await expect.poll(async () => (await metrics(page))?.enabled).toBe(true);
});

test("Ctrl right-click queues orders and F3 toggles developer tools", async ({
  page,
}, info) => {
  test.setTimeout(90_000);
  const renderer = info.project.name === "webgpu" ? "webgpu" : "webgl2";
  const commands = await inputFixture(page, renderer);
  const overlay = page.locator("#performance-overlay");
  await page.mouse.click(640, 360);
  await page.keyboard.down("Control");
  await page.mouse.click(850, 430, { button: "right" });
  await page.keyboard.up("Control");
  await expect.poll(() => commands.length).toBe(1);
  expect(commands[0]?.[0]).toBe(21);
  await expect(overlay).toBeHidden();
  await page.keyboard.press("F3");
  await expect(overlay).toBeVisible();
  await page.keyboard.press("F3");
  await expect(overlay).toBeHidden();
});

test("normal player navigation keeps developer timing disabled", async ({
  page,
}, info) => {
  const renderer = info.project.name === "webgpu" ? "webgpu" : "webgl2";
  await inputFixture(page, renderer);
  await expect(page.locator("#performance-overlay")).toBeHidden();
  expect((await metrics(page))?.enabled).toBe(false);
  await page.evaluate(() => {
    location.hash = "x=8192.5&y=8192.5&zoom=.04";
  });
  await expect(page.locator("#scene")).toHaveAttribute("data-lod", "markers");
  await expect(page.locator("#performance-overlay")).toBeHidden();
  expect((await metrics(page))?.enabled).toBe(false);
  expect((await metrics(page))?.totalsMs).toEqual({
    decode: 0,
    upload: 0,
    draw: 0,
    ui: 0,
  });
});

for (const delay of ["slow callback", "late browser dispatch"] as const) {
  test(`game frames yield to queued browser tasks after a ${delay}`, async ({
    page,
  }, info) => {
    await inputFixture(
      page,
      info.project.name === "webgpu" ? "webgpu" : "webgl2",
    );
    const taskRan = await page.evaluate(
      (delay) =>
        new Promise<boolean>((resolve) => {
          const timing = Reflect.get(window, "aoePerformance") as {
            mark(section: number): void;
          };
          const mark = timing.mark;
          const requestFrame = window.requestAnimationFrame;
          const channel = new MessageChannel();
          let delayRequested = false;
          let delayedFrame = false;
          let observedFrame = false;
          let finished = false;
          let ran = false;
          channel.port1.onmessage = () => {
            ran = true;
          };
          timing.mark = (section) => {
            mark(section);
            if (section === 0 && delay === "slow callback" && !observedFrame) {
              observedFrame = true;
              const until = performance.now() + 60;
              while (performance.now() < until) {
                // Simulate one over-budget render callback.
              }
            }
            if (section === 21 && (observedFrame || delayedFrame)) {
              finished = true;
              channel.port2.postMessage(null);
            }
          };
          window.requestAnimationFrame = (callback) => {
            if (finished) {
              timing.mark = mark;
              window.requestAnimationFrame = requestFrame;
              channel.port1.close();
              channel.port2.close();
              resolve(ran);
            } else if (delay === "late browser dispatch" && !delayRequested) {
              delayRequested = true;
              window.setTimeout(() => {
                requestFrame.call(window, (time) => {
                  delayedFrame = true;
                  callback(time);
                });
              }, 80);
              return 0;
            }
            return requestFrame.call(window, callback);
          };
        }),
      delay,
    );
    expect(taskRan).toBe(true);
  });
}

test("frame telemetry runs with the overlay hidden and posts bounded stage summaries", async ({
  page,
}, info) => {
  const reports: Record<string, unknown>[] = [];
  await page.route("**/client/perf", async (route) => {
    reports.push(route.request().postDataJSON() as Record<string, unknown>);
    await route.fulfill({ status: 204 });
  });
  await inputFixture(
    page,
    info.project.name === "webgpu" ? "webgpu" : "webgl2",
  );
  await expect(page.locator("#performance-overlay")).toBeHidden();
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-view-painted",
    "true",
  );
  expect(reports).toHaveLength(0);
  await page.evaluate(() => {
    const now = performance.now.bind(performance);
    performance.now = () => now() + 60_000;
  });
  await expect
    .poll(() => reports.length, { timeout: 15000 })
    .toBeGreaterThan(0);
  const report = reports[0];
  if (!report) throw new Error("No telemetry report arrived");
  expect(report["frames"]).toBeGreaterThan(0);
  expect(report["renderer"]).toBe(
    info.project.name === "webgpu" ? "webgpu" : "webgl2",
  );
  expect(report["units_in_view"]).toBeGreaterThan(0);
  const still = report["still"] as {
    frames: number;
    over_50_ms: number;
    over_100_ms: number;
  };
  expect(still.frames).toBeGreaterThan(0);
  expect(still.over_50_ms).toBeGreaterThanOrEqual(still.over_100_ms);
  for (const values of [
    report["frame_ms"],
    report["draw_calls"],
    ...Object.values(report["stages_ms"] as object),
  ]) {
    const stats = values as number[];
    expect(stats).toHaveLength(3);
    expect(stats.every(Number.isFinite)).toBe(true);
    const [p50, tail, max] = stats;
    if (p50 === undefined || tail === undefined || max === undefined)
      throw new Error("Missing timing quantiles");
    expect(p50).toBeGreaterThanOrEqual(0);
    expect(tail).toBeGreaterThanOrEqual(p50);
    expect(max).toBeGreaterThanOrEqual(tail);
  }
  expect(Object.keys(report["stages_ms"] as object).sort()).toEqual(
    [
      "update",
      "interpolation",
      "culling",
      "terrain",
      "sprites",
      "ui",
      "fog",
    ].sort(),
  );
  expect((report["draw_calls"] as number[])[2]).toBeGreaterThan(0);
  const contour = report["contour"] as Record<string, number>;
  expect(Object.keys(contour).sort()).toEqual(
    [
      "context_lost",
      "context_restored",
      "depth_span",
      "effect_frames",
      "eligible_instances",
      "occluder_instances",
      "plain_frames",
    ].sort(),
  );
  expect(Object.values(contour).every(Number.isFinite)).toBe(true);
  // Every submitted frame is counted as effects-on or effects-off.
  expect(
    (contour["effect_frames"] ?? 0) + (contour["plain_frames"] ?? 0),
  ).toBeGreaterThan(0);
  expect(contour["context_lost"]).toBe(0);
});

test("summary percentiles retain stalls and stage totals without unbounded history", async ({
  page,
}) => {
  await page.goto("/diagnostics.html");
  const { result, boundaries } = await page.evaluate(async () => {
    const { frameTelemetry } = (await import(
      "/dev/frame-telemetry.js" as string
    )) as {
      frameTelemetry: (clock: () => number) => {
        mark: (section: number) => void;
        summary: () => {
          frames: number;
          frame_ms: number[];
          stages_ms: Record<string, number[]>;
          panning: {
            frames: number;
            over_50_ms: number;
            over_100_ms: number;
            max_ms: number;
          };
          still: { frames: number };
        };
        view: (x: number, y: number) => boolean;
      };
    };
    let time = 1;
    const telemetry = frameTelemetry(() => time);
    for (let index = 0; index <= 33_000; index++) {
      telemetry.mark(0);
      telemetry.mark(7);
      time += 3;
      telemetry.mark(14);
      telemetry.view(index < 33_000 ? 0 : 1, 0);
      telemetry.mark(21);
      time += index === 32_999 ? 997 : index % 10 === 0 ? 77 : 13;
    }
    const boundaries = frameTelemetry(() => time);
    for (const delta of [0, 50, 50.125, 100, 100.125]) {
      time += delta;
      boundaries.mark(0);
      boundaries.view(1, 1);
      boundaries.mark(21);
    }
    return { result: telemetry.summary(), boundaries: boundaries.summary() };
  });
  expect(result.frames).toBe(33_000);
  expect(result.frame_ms).toEqual([16, 80, 1000]);
  expect(result.still.frames).toBe(32_999);
  expect(result.still).toMatchObject({
    over_50_ms: 3300,
    over_100_ms: 0,
  });
  expect(result.panning).toMatchObject({
    frames: 1,
    over_50_ms: 1,
    over_100_ms: 1,
    max_ms: 1000,
  });
  expect(result.stages_ms["update"]).toEqual([3, 3, 3]);
  expect(boundaries.frame_ms).toEqual([50.125, 100.125, 100.125]);
  expect(boundaries.still).toMatchObject({
    frames: 4,
    over_50_ms: 3,
    over_100_ms: 1,
  });
});

test("normal HUD work follows visible actors while selected offscreen actors and global counts survive camera jumps", async ({
  page,
}, info) => {
  const renderer = info.project.name === "webgpu" ? "webgpu" : "webgl2";
  await inputFixture(page, renderer, [], false, 0, undefined, {
    offscreenUnits: 1000,
  });
  const actors = () =>
    page
      .locator("#scene")
      .getAttribute("data-actors")
      .then(
        (value) =>
          JSON.parse(value ?? "[]") as { id: number; selected: boolean }[],
      );
  await expect.poll(async () => (await actors()).length).toBeLessThan(10);
  await expect(page.locator("#idle-villager-count")).toHaveText("1002");
  const sceneChecks = await page.evaluate(
    () =>
      new Promise<number>((resolve) => {
        const { strategic } = Reflect.get(window, "aoeNavigation") as {
          strategic: { cinematic: () => boolean };
        };
        const hud = Reflect.get(window, "aoeInterface") as {
          numeric: (values: Float64Array, status: string) => void;
        };
        const { cinematic } = strategic;
        const { numeric } = hud;
        let calls = 0;
        let updates = 0;
        strategic.cinematic = () => {
          calls++;
          return cinematic();
        };
        hud.numeric = (values, status) => {
          numeric(values, status);
          if (++updates === 2) {
            strategic.cinematic = cinematic;
            hud.numeric = numeric;
            resolve(calls);
          }
          calls = 0;
        };
      }),
  );
  expect(sceneChecks).toBeLessThan(120);
  const population = await page.locator("#population").textContent();
  await selectWorker(page);
  await page.evaluate(() => {
    location.hash = "x=16000&y=16000&zoom=.6";
  });
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-camera-x",
    "16000",
  );
  await expect
    .poll(async () => (await actors()).map((actor) => actor.id))
    .toEqual([2]);
  expect((await actors())[0]?.selected).toBe(true);
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "1");
  await expect(page.locator("#idle-villager-count")).toHaveText("1002");
  await expect(page.locator("#population")).toHaveText(population ?? "");
  await page.evaluate(() => {
    location.hash = "x=8192.5&y=8192.5&zoom=.6";
  });
  await expect
    .poll(async () => (await actors()).some((actor) => actor.id === 3))
    .toBe(true);
});

test("current view waits for its subscription snapshot before reporting painted", async ({
  page,
}, info) => {
  const renderer = info.project.name === "webgpu" ? "webgpu" : "webgl2";
  let release!: () => void;
  let held = new Promise<void>((resolve) => {
    release = resolve;
  });
  const opening = inputFixture(page, renderer, [], false, 0, undefined, {
    beforeSnapshot: () => held,
  });
  try {
    await expect(page.locator("#connection")).toHaveText("connected");
    await expect
      .poll(async () =>
        Number(await page.locator("#scene").getAttribute("data-frame")),
      )
      .toBeGreaterThan(0);
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-view-painted",
      "false",
    );
  } finally {
    release();
    await opening;
  }
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-view-painted",
    "true",
  );
  held = new Promise<void>((resolve) => {
    release = resolve;
  });
  try {
    await page.evaluate(() => {
      location.hash = "x=8210&y=8210&zoom=.6";
    });
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-view-painted",
      "false",
    );
  } finally {
    release();
  }
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-view-painted",
    "true",
  );
  held = new Promise<void>((resolve) => {
    release = resolve;
  });
  try {
    await page.locator("#reconnect").evaluate((button) => {
      (button as HTMLButtonElement).click();
    });
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-view-painted",
      "false",
    );
  } finally {
    release();
  }
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-view-painted",
    "true",
  );
});
