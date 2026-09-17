import { expect, test } from "@playwright/test";
import { PNG } from "pngjs";
import { mkdir, writeFile } from "node:fs/promises";

test("software WebGPU renders a synthetic scene and camera moves", async ({
  page,
}) => {
  await page.goto("/");
  const diagnostics = page.getByRole("status");
  await expect(diagnostics).toContainText("connected");
  await expect(diagnostics).toContainText(/adapter: .*WebGpu/);
  await expect(diagnostics).toContainText("scenario: smoke");
  await expect(diagnostics).toContainText("tick:");
  await expect(diagnostics).toContainText(/visible: [1-9]/);
  const screenshot = await page.locator("#scene").screenshot();
  const image = PNG.sync.read(screenshot);
  let brightPixels = 0;
  for (let i = 0; i < image.data.length; i += 4) {
    const r = image.data[i] ?? 0;
    const g = image.data[i + 1] ?? 0;
    const b = image.data[i + 2] ?? 0;
    if (r > 100 || g > 100 || b > 100) brightPixels += 1;
  }
  expect(brightPixels).toBeGreaterThan(20);
  await page.keyboard.press("ArrowRight");
  await expect(diagnostics).toContainText("camera: 32,0");
  await page.keyboard.press("+");
  await expect(diagnostics).toContainText("zoom: 5.00");
});

test("native and browser replay hashes agree", async ({ page }) => {
  await page.goto("/");
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
  await page.goto("/");
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
  await page.goto("/");
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
    await page.waitForTimeout(1_000);
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
  await Promise.all([first.goto("/"), second.goto("/")]);
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
  await page.goto("/");
  await expect(page.getByRole("alert")).toContainText("WebGPU unavailable");
  await context.close();
});
