import { expect, test } from "@playwright/test";
import { PNG } from "pngjs";
import { mkdir, writeFile } from "node:fs/promises";

test("software WebGPU renders a synthetic scene and camera moves", async ({
  page,
}, testInfo) => {
  await page.goto("/diagnostics.html");
  const diagnostics = page.getByRole("status");
  await expect(diagnostics).toContainText("connected");
  await expect(diagnostics).toContainText(/adapter: .*WebGpu/);
  await expect(diagnostics).toContainText("scenario: smoke");
  await expect(diagnostics).toContainText("tick:");
  await expect(diagnostics).toContainText(/visible: [1-9]/);
  const canvas = page.locator("#scene");
  const box = await canvas.boundingBox();
  if (!box) throw new Error("scene canvas is missing");
  const screenshot = await page.screenshot({ fullPage: true });
  await testInfo.attach("smoke-webgpu.png", {
    body: screenshot,
    contentType: "image/png",
  });
  const image = PNG.sync.read(screenshot);
  const left = Math.round(box.x);
  const top = Math.round(box.y);
  const width = Math.round(box.width);
  const height = Math.round(box.height);
  expect(left + width).toBeLessThanOrEqual(image.width);
  expect(top + height).toBeLessThanOrEqual(image.height);
  const colors = { blue: 0, orange: 0, green: 0, yellow: 0 };
  let background = 0;
  for (let y = top; y < top + height; y += 1) {
    for (let x = left; x < left + width; x += 1) {
      const i = (y * image.width + x) * 4;
      const r = image.data[i] ?? 0;
      const g = image.data[i + 1] ?? 0;
      const b = image.data[i + 2] ?? 0;
      if (r < 40 && g < 40 && b < 50) background += 1;
      if (b > 150 && b > r + 50 && b > g + 20) colors.blue += 1;
      if (r > 200 && g > 80 && g < 190 && b < 150) colors.orange += 1;
      if (g > 150 && g > r + 30 && g > b + 20) colors.green += 1;
      if (r > 180 && g > 140 && b < 130) colors.yellow += 1;
    }
  }
  expect(background).toBeGreaterThan(width * height * 0.7);
  for (const count of Object.values(colors)) expect(count).toBeGreaterThan(100);
  await page.keyboard.press("ArrowRight");
  await expect(diagnostics).toContainText("camera: 32,0");
  await page.keyboard.press("+");
  await expect(diagnostics).toContainText("zoom: 5.00");
});

test("native and browser replay hashes agree", async ({ page }) => {
  await page.goto("/diagnostics.html");
  const response = await page.request.get(
    "/replay-hash?scenario=smoke&ticks=20",
  );
  expect(response.ok()).toBe(true);
  const native = (await response.json()) as { hash: string };
  const wasm = await page.evaluate(async () => {
    const module = (await import(
      new URL("/pkg/aoe_client.js", location.href).href
    )) as {
      replay_hash(scenario: string, ticks: number): string;
    };
    return module.replay_hash("smoke", 20);
  });
  expect(wasm).toBe(native.hash);
});

test("scenario control changes the authoritative world and rejects invalid names", async ({
  page,
}) => {
  await page.goto("/diagnostics.html");
  const diagnostics = page.getByRole("status");
  await expect(diagnostics).toContainText("connected");
  try {
    await page.getByLabel("Scenario").selectOption("target-hotspot");
    await expect(diagnostics).toContainText("scenario: target-hotspot");
    await expect(diagnostics).toContainText("/ 128000");
    const invalid = await page.request.post("/scenario/not-a-scenario");
    expect(invalid.status()).toBe(400);
  } finally {
    await page.getByLabel("Scenario").selectOption("smoke");
    await expect(diagnostics).toContainText("scenario: smoke");
  }
});

test("hotspot camera churn retains bounded browser timing and resources", async ({
  page,
}, testInfo) => {
  test.setTimeout(90_000);
  await page.setViewportSize({ width: 1920, height: 1080 });
  await page.goto("/diagnostics.html");
  const diagnostics = page.getByRole("status");
  await expect(diagnostics).toContainText("connected");
  try {
    await page.getByLabel("Scenario").selectOption("target-hotspot");
    await expect(diagnostics).toContainText("scenario: target-hotspot");
    await expect(diagnostics).toContainText(/visible: 1[0-6][0-9]{3}/);
    const read = () =>
      page.evaluate(async () => {
        const module = (await import(
          new URL("/pkg/aoe_client.js", location.href).href
        )) as {
          performance_snapshot(): unknown;
          reset_performance_samples(): void;
        };
        return module.performance_snapshot() as {
          version: number;
          scenario: string;
          viewport_width: number;
          viewport_height: number;
          total_entities: number;
          resident_entities: number;
          maximum_visible: number;
          frames: number;
          messages: number;
          frame_intervals_ms: number[];
          cpu_submission_ms: number[];
          decode_update_ms: number[];
          wasm_memory_bytes: number | null;
          first_visible_ms: number | null;
          gpu_buffer_bytes: number;
          persistent_gpu_resources: number;
          atlas_pages: number;
          atlas_uploads: number;
          atlas_bytes: number;
        };
      });
    await page.evaluate(async () => {
      const module = (await import(
        new URL("/pkg/aoe_client.js", location.href).href
      )) as { reset_performance_samples(): void };
      module.reset_performance_samples();
    });
    const before = await read();
    for (let cycle = 0; cycle < 3; cycle += 1) {
      for (let step = 0; step < 8; step += 1)
        await page.keyboard.press("ArrowRight");
      for (let step = 0; step < 8; step += 1)
        await page.keyboard.press("ArrowLeft");
      await page.keyboard.press("+");
      await page.keyboard.press("-");
    }
    await expect(diagnostics).toContainText("camera: 0,0");
    await expect(diagnostics).toContainText(/visible: 1[0-6][0-9]{3}/);
    await expect
      .poll(async () => (await read()).frames, { timeout: 15_000 })
      .toBeGreaterThan(12);
    const after = await read();
    expect(after.version).toBe(1);
    expect(after.scenario).toBe("target-hotspot");
    expect(after.viewport_width).toBe(1920);
    expect(after.viewport_height).toBeGreaterThan(900);
    expect(after.total_entities).toBe(128_000);
    expect(after.resident_entities).toBeGreaterThanOrEqual(10_000);
    expect(after.maximum_visible).toBeGreaterThanOrEqual(10_000);
    expect(after.frames).toBeGreaterThan(10);
    expect(after.messages).toBeGreaterThan(4);
    expect(after.frame_intervals_ms.length).toBeGreaterThan(10);
    expect(after.cpu_submission_ms.length).toBeGreaterThan(10);
    expect(after.decode_update_ms.length).toBeGreaterThan(4);
    expect(after.wasm_memory_bytes).toBeGreaterThan(0);
    expect(after.first_visible_ms ?? -1).toBeGreaterThan(0);
    expect(after.gpu_buffer_bytes).toBe(before.gpu_buffer_bytes);
    expect(after.persistent_gpu_resources).toBe(
      before.persistent_gpu_resources,
    );
    expect(after.atlas_pages).toBe(before.atlas_pages);
    expect(after.atlas_uploads).toBe(before.atlas_uploads);
    expect(after.atlas_pages).toBe(1);
    expect(after.atlas_uploads).toBe(1);
    expect(after.atlas_bytes).toBe(256);
    const report = JSON.stringify(
      {
        version: 1,
        evidence_class: "hosted_informational",
        verdict: "INCONCLUSIVE",
        browser: await page.evaluate(() => navigator.userAgent),
        before,
        after,
      },
      null,
      2,
    );
    const reportPath = new URL(
      "../../reports/perf/browser.json",
      import.meta.url,
    );
    await mkdir(new URL("../../reports/perf/", import.meta.url), {
      recursive: true,
    });
    await writeFile(reportPath, report);
    await testInfo.attach("hotspot-browser-timing.json", {
      body: report,
      contentType: "application/json",
    });
  } finally {
    await page.getByLabel("Scenario").selectOption("smoke");
    await expect(diagnostics).toContainText("scenario: smoke");
  }
});

test("two sessions subscribe independently and reconnect", async ({
  browser,
}) => {
  const first = await browser.newPage();
  const second = await browser.newPage();
  await Promise.all([
    first.goto("/diagnostics.html"),
    second.goto("/diagnostics.html"),
  ]);
  await expect(first.getByRole("status")).toContainText("connected");
  await expect(second.getByRole("status")).toContainText("connected");
  for (let i = 0; i < 16; i += 1) await second.keyboard.press("ArrowRight");
  await expect(second.getByRole("status")).toContainText("camera: 512,0");
  await expect(first.getByRole("status")).toContainText("camera: 0,0");
  await second.getByRole("button", { name: "Reconnect" }).click();
  await expect(second.getByRole("status")).toContainText("connected");
  await expect(second.getByRole("status")).toContainText("connection: 2");
  await expect(second.getByRole("status")).toContainText(/resident: [1-9]/);
  await Promise.all([first.close(), second.close()]);
});

test("unsupported WebGPU shows a capability error", async ({ browser }) => {
  const context = await browser.newContext();
  await context.addInitScript(() => {
    Object.defineProperty(navigator, "gpu", { value: undefined });
  });
  const page = await context.newPage();
  await page.goto("/diagnostics.html");
  await expect(page.getByRole("alert")).toContainText("WebGPU unavailable");
  await context.close();
});

test("unknown URL scenario shows visible configuration feedback", async ({
  page,
}) => {
  await page.goto("/diagnostics.html?scenario=invalid-demo");
  await expect(page.getByRole("alert")).toContainText(
    "Unknown scenario configuration",
  );
  await expect(page.getByRole("status")).toContainText("connected");
  await expect(page.getByRole("status")).toContainText("scenario: smoke");
});
