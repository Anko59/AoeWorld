import { expect, test } from "@playwright/test";
import { PNG } from "pngjs";

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
