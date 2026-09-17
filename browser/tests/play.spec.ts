import { expect, test } from "@playwright/test";
import { gameAssets } from "./game-assets.js";
import { PNG } from "pngjs";

test("AoE II scout moves to clicks, redirects, resets, and survives resizing", async ({
  page,
}, testInfo) => {
  const evidence = await gameAssets(page);
  testInfo.annotations.push({ type: "assets", description: evidence });
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/");
  const status = page.getByRole("status");
  const position = page.locator("#position");
  const canvas = page.locator("canvas");
  await expect(status).toHaveText("Ready", { timeout: 30_000 });
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-assets",
    "aoe2-local",
  );
  await expect(position).toHaveText("480, 320");
  const move = async (x: number, y: number) => {
    const box = await canvas.boundingBox();
    if (!box) throw new Error("Missing map");
    await canvas.click({
      position: { x: (box.width * x) / 960, y: (box.height * y) / 640 },
    });
  };
  const screenshot = await canvas.screenshot({
    path: "../reports/e2e/aoeworld-map.png",
  });
  await page.screenshot({
    path: "../reports/e2e/aoeworld.png",
    fullPage: true,
  });
  const image = PNG.sync.read(screenshot);
  let blue = 0;
  let grass = 0;
  for (let i = 0; i < image.data.length; i += 4) {
    const r = image.data[i] ?? 0;
    const g = image.data[i + 1] ?? 0;
    const b = image.data[i + 2] ?? 0;
    if (b > 180 && b > r + 50) blue += 1;
    if (g > r + 15 && g > b + 10) grass += 1;
  }
  expect(blue).toBeGreaterThan(30);
  expect(grass).toBeGreaterThan(image.width * image.height * 0.7);
  await testInfo.attach("first-scout.png", {
    body: screenshot,
    contentType: "image/png",
  });
  await move(760, 440);
  await expect(status).toHaveText("Moving");
  await expect(position).not.toHaveText("480, 320");
  await move(600, 220);
  await expect(status).toHaveText("Ready");
  const coordinates = (await position.innerText()).split(",").map(Number);
  expect(Math.abs((coordinates[0] ?? 0) - 600)).toBeLessThanOrEqual(2);
  expect(Math.abs((coordinates[1] ?? 0) - 220)).toBeLessThanOrEqual(2);
  expect(await canvas.screenshot()).not.toEqual(screenshot);
  await page.getByRole("button", { name: "Reset position" }).click();
  await expect(position).toHaveText("480, 320");
  await canvas.focus();
  await page.keyboard.press("ArrowLeft");
  await expect(position).toHaveText("420, 320");
  await page.setViewportSize({ width: 540, height: 820 });
  await move(540, 360);
  await expect(status).toHaveText("Moving");
  await expect(status).toHaveText("Ready");
  const resized = (await position.innerText()).split(",").map(Number);
  expect(Math.abs((resized[0] ?? 0) - 540)).toBeLessThanOrEqual(3);
  expect(Math.abs((resized[1] ?? 0) - 360)).toBeLessThanOrEqual(3);
  expect(errors).toEqual([]);
});

test("playground reports unavailable WebGPU", async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "gpu", { value: undefined });
  });
  await page.goto("/");
  await expect(page.getByRole("alert")).toContainText("WebGPU unavailable");
});

test("missing local assets show an actionable error", async ({ page }) => {
  await page.route("**/asset-pack/manifest.json", (route) =>
    route.fulfill({ status: 404 }),
  );
  await page.goto("/");
  await expect(page.getByRole("alert")).toContainText("local asset pack");
});
