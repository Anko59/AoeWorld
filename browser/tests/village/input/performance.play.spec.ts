import { expect, type Page } from "@playwright/test";
import { test } from "../../../server.js";
import { inputFixture } from "./fixture.js";

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
  await expect
    .poll(() => reports.length, { timeout: 15000 })
    .toBeGreaterThan(0);
  const report = reports[0];
  if (!report) throw new Error("No telemetry report arrived");
  expect(report["frames"]).toBeGreaterThan(0);
  expect(report["frames"]).toBeLessThanOrEqual(2048);
  for (const values of [
    report["frame_ms"],
    report["draw_calls"],
    ...Object.values(report["stages_ms"] as object),
  ]) {
    const stats = values as number[];
    expect(stats).toHaveLength(3);
    expect(stats.every(Number.isFinite)).toBe(true);
    const [p50, p95, max] = stats;
    if (p50 === undefined || p95 === undefined || max === undefined)
      throw new Error("Missing timing quantiles");
    expect(p50).toBeGreaterThanOrEqual(0);
    expect(p95).toBeGreaterThanOrEqual(p50);
    expect(max).toBeGreaterThanOrEqual(p95);
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
});

test("summary percentiles retain stalls and stage totals without unbounded history", async ({
  page,
}) => {
  await page.goto("/diagnostics.html");
  const result = await page.evaluate(async () => {
    const { frameTelemetry } = (await import(
      "/dev/frame-telemetry.js" as string
    )) as {
      frameTelemetry: (clock: () => number) => {
        mark: (section: number) => void;
        summary: () => {
          frames: number;
          frame_ms: number[];
          stages_ms: Record<string, number[]>;
        };
      };
    };
    let time = 1;
    const telemetry = frameTelemetry(() => time);
    for (let index = 0; index < 2100; index++) {
      telemetry.mark(0);
      telemetry.mark(7);
      time += 3;
      telemetry.mark(14);
      telemetry.mark(21);
      time += index === 2098 ? 997 : index % 10 === 0 ? 77 : 13;
    }
    return telemetry.summary();
  });
  expect(result.frames).toBe(2048);
  expect(result.frame_ms).toEqual([16, 80, 1000]);
  expect(result.stages_ms["update"]).toEqual([3, 3, 3]);
});
