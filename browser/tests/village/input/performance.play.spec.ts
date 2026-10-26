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

test("Ctrl right-click toggles developer tools while Shift keeps queued orders", async ({
  page,
}, info) => {
  test.setTimeout(90_000);
  const renderer = info.project.name === "webgpu" ? "webgpu" : "webgl2";
  const commands = await inputFixture(page, renderer);
  const overlay = page.locator("#performance-overlay");
  await page.mouse.click(640, 360);
  await page.keyboard.down("Shift");
  await page.mouse.click(850, 430, { button: "right" });
  await page.keyboard.up("Shift");
  await expect.poll(() => commands.length).toBe(1);
  expect(commands[0]?.[0]).toBe(21);
  await expect(overlay).toBeHidden();
  const count = commands.length;
  await page.keyboard.down("Control");
  await page.mouse.click(850, 430, { button: "right" });
  await page.keyboard.up("Control");
  await expect(overlay).toBeVisible();
  expect(commands.length).toBe(count);
  await page.keyboard.down("Control");
  await page.mouse.click(850, 430, { button: "right" });
  await page.keyboard.up("Control");
  await expect(overlay).toBeHidden();
});
