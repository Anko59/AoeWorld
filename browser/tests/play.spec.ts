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
  if (testInfo.project.name === "webgpu") {
    await expect(page.locator("#playground")).toHaveAttribute(
      "data-renderer",
      "webgpu",
    );
  }
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

for (const failure of ["missing-api", "null-context", "no-adapter"] as const) {
  test(`game remains playable after WebGPU ${failure}`, async ({ page }) => {
    await gameAssets(page);
    await page.addInitScript((mode) => {
      if (mode === "missing-api") {
        Object.defineProperty(navigator, "gpu", { value: undefined });
      } else if (mode === "no-adapter") {
        const gpu = (navigator as Navigator & { gpu?: object }).gpu;
        if (gpu)
          Object.defineProperty(gpu, "requestAdapter", {
            value: async () => null,
          });
      } else {
        const original = HTMLCanvasElement.prototype.getContext;
        HTMLCanvasElement.prototype.getContext = function (
          this: HTMLCanvasElement,
          type: string,
          ...args: unknown[]
        ) {
          if (type === "webgpu") return null;
          return Reflect.apply(original, this, [type, ...args]);
        } as typeof original;
      }
    }, failure);
    await page.goto("/");
    await expect(page.getByRole("status")).toHaveText("Ready", {
      timeout: 30_000,
    });
    await expect(page.locator("#playground")).toHaveAttribute(
      "data-renderer",
      "canvas2d",
    );
    await expect(page.getByRole("alert")).toBeEmpty();
    const canvas = page.locator("canvas");
    const image = PNG.sync.read(await canvas.screenshot());
    let colored = 0;
    for (let i = 0; i < image.data.length; i += 4) {
      if ((image.data[i + 1] ?? 0) > (image.data[i] ?? 0) + 15) colored += 1;
    }
    expect(colored).toBeGreaterThan(image.width * image.height * 0.7);
    await canvas.click({ position: { x: 40, y: 90 } });
    await expect(page.getByRole("status")).toHaveText("Moving");
    await expect(page.locator("#position")).not.toHaveText("480, 320");
    await expect(page.getByRole("status")).toHaveText("Ready");
    await page.getByRole("button", { name: "Reset position" }).click();
    await expect(page.locator("#position")).toHaveText("480, 320");
  });
}

test("missing local assets show an actionable error", async ({ page }) => {
  await page.route("**/asset-pack/manifest.json", (route) =>
    route.fulfill({ status: 404 }),
  );
  await page.goto("/");
  await expect(page.getByRole("alert")).toContainText("local asset pack");
  await expect(page.getByRole("status")).toHaveText("Unavailable");
  await expect(
    page.getByRole("button", { name: "Reset position" }),
  ).toBeDisabled();
});

test("unavailable renderers stop loading and disable controls", async ({
  page,
}) => {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "gpu", { value: undefined });
    HTMLCanvasElement.prototype.getContext = () => null;
  });
  await page.goto("/");
  await expect(page.getByRole("status")).toHaveText("Unavailable");
  await expect(page.getByRole("alert")).toContainText(
    "Canvas 2D is unavailable",
  );
  await expect(
    page.getByRole("button", { name: "Reset position" }),
  ).toBeDisabled();
});
