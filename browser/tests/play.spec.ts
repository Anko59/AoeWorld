import { expect, test, type Page } from "@playwright/test";
import { gameAssets } from "./game-assets.js";
import { PNG } from "pngjs";

async function waitForGame(page: Page) {
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-assets",
    "aoe2-local",
  );
}

async function bluePixels(canvas: ReturnType<Page["locator"]>) {
  const image = PNG.sync.read(await canvas.screenshot());
  let blue = 0;
  for (let i = 0; i < image.data.length; i += 4) {
    const r = image.data[i] ?? 0;
    const g = image.data[i + 1] ?? 0;
    const b = image.data[i + 2] ?? 0;
    if (b > 150 && b > r + 30 && b > g + 30) blue += 1;
  }
  return blue;
}

test.beforeEach(async ({ request }) => {
  const response = await request.post("/maps/reset");
  expect(response.status()).toBe(204);
});

test("authoritative isometric game renders, selects, orders, and survives reload", async ({
  page,
}, testInfo) => {
  const evidence = await gameAssets(page);
  testInfo.annotations.push({ type: "assets", description: evidence });
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/");
  await waitForGame(page);
  if (testInfo.project.name === "webgpu") {
    await expect(page.locator("#playground")).toHaveAttribute(
      "data-renderer",
      "webgpu",
    );
  }
  const canvas = page.locator("#scene");
  await expect
    .poll(() => bluePixels(canvas), { timeout: 10_000 })
    .toBeGreaterThan(10);
  const first = await canvas.screenshot({
    path: "../reports/e2e/aoeworld-map.png",
  });
  const image = PNG.sync.read(first);
  let green = 0;
  let blue = 0;
  let terrainTiles = 0;
  for (let i = 0; i < image.data.length; i += 4) {
    const r = image.data[i] ?? 0;
    const g = image.data[i + 1] ?? 0;
    const b = image.data[i + 2] ?? 0;
    if (g > r + 8 && g > b + 4) green += 1;
    if (b > 150 && b > r + 30) blue += 1;
    if (r >= 60 && r <= 80 && g > 100 && b >= 40 && b <= 70) terrainTiles += 1;
  }
  expect(green).toBeGreaterThan(image.width * image.height * 0.6);
  expect(blue).toBeGreaterThan(10);
  expect(terrainTiles).toBeGreaterThan(image.width * image.height * 0.1);

  const box = await canvas.boundingBox();
  if (!box) throw new Error("Missing map");
  await canvas.hover({
    position: { x: box.width / 2, y: box.height / 2 },
  });
  await expect(page.locator("#tile-inspection")).toContainText(
    "tile 8192, 8192",
  );
  await canvas.click({ position: { x: box.width / 2, y: box.height / 2 } });
  await canvas.click({
    position: { x: box.width / 2 + 110, y: box.height / 2 + 30 },
    button: "right",
  });
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 10_000,
  });
  await page.getByRole("button", { name: "Toggle isometric grid" }).click();
  await page.getByRole("button", { name: "Center on primary unit" }).click();
  await page.reload();
  await waitForGame(page);
  expect(errors).toEqual([]);
});

test("map creator pans, zooms, and preserves preview-only fallback maps", async ({
  page,
}) => {
  await gameAssets(page);
  await page.goto("/");
  await waitForGame(page);
  await page.getByRole("button", { name: "Open map creator" }).click();
  await expect(page.getByLabel("Region")).toHaveValue("paris");
  await expect(page.locator("#map-coordinates")).toBeHidden();
  await expect(page.locator("#map-location")).toContainText("Paris Basin");
  await expect(page.locator("#map-footprint")).toHaveAttribute(
    "visibility",
    "visible",
  );
  await expect(page.locator("#map-footprint")).toHaveAttribute("d", /M/);
  const overview = page.locator("#map-overview");
  const overviewBox = await overview.boundingBox();
  if (!overviewBox) throw new Error("world overview is missing");
  await overview.hover({
    position: { x: overviewBox.width / 2, y: overviewBox.height / 2 },
  });
  await page.mouse.wheel(0, -120);
  await expect(overview).not.toHaveAttribute("viewBox", "0 0 360 180");
  const zoomedView = await overview.getAttribute("viewBox");
  if (!zoomedView) throw new Error("zoom did not set an overview viewBox");
  await page.keyboard.down("Shift");
  await page.mouse.move(
    overviewBox.x + overviewBox.width / 2,
    overviewBox.y + overviewBox.height / 2,
  );
  await page.mouse.down();
  await page.mouse.move(
    overviewBox.x + overviewBox.width / 2 + 24,
    overviewBox.y + overviewBox.height / 2,
  );
  await page.mouse.up();
  await page.keyboard.up("Shift");
  await expect(overview).not.toHaveAttribute("viewBox", zoomedView);
  await overview.dblclick({
    position: { x: overviewBox.width / 2, y: overviewBox.height / 2 },
  });
  await page.mouse.move(
    overviewBox.x + overviewBox.width * 0.72,
    overviewBox.y + overviewBox.height * 0.32,
  );
  await page.mouse.down();
  await page.mouse.move(
    overviewBox.x + overviewBox.width * 0.74,
    overviewBox.y + overviewBox.height * 0.34,
  );
  await page.mouse.up();
  await expect(page.getByLabel("Region")).toHaveValue("custom");
  await expect(page.locator("#map-coordinates")).toBeVisible();
  await page.getByLabel("Region").selectOption("nile");
  await expect(page.locator("#map-location")).toContainText("Nile Delta");
  await page.getByLabel("Region").selectOption("fiji");
  await expect(page.locator("#map-location")).toContainText("Fiji");
  await expect(page.getByLabel("Latitude")).toHaveValue("-17.8");
  await expect(page.getByLabel("Longitude")).toHaveValue("179.8");
  await page.getByLabel("Region").selectOption("antarctic");
  await expect(page.locator("#map-location")).toContainText(
    "Antarctic interior",
  );
  await page.getByLabel("Region").selectOption("custom");
  await expect(page.locator("#map-coordinates")).toBeVisible();
  await page.getByLabel("Latitude").fill("48.8566");
  await page.getByLabel("Longitude").fill("2.3522");
  await page.getByLabel("Square km").fill("0.256");
  await page.getByLabel("Compression").fill("2");
  await page.getByRole("button", { name: "Estimate" }).click();
  await expect(page.locator("#map-estimate")).toContainText("64 × 64 tiles");
  await expect(page.getByRole("button", { name: "Generate" })).toBeEnabled();
  await page.getByRole("button", { name: "Generate" }).click();
  await expect(page.locator("#map-estimate")).toContainText(
    "Fallback package available for preview",
  );
  await expect(page.locator("#map-estimate")).toContainText(
    "no suitable land start.",
  );
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
    await waitForGame(page);
    await expect(page.locator("#playground")).toHaveAttribute(
      "data-renderer",
      "canvas2d",
    );
    await expect(page.getByRole("alert")).toBeEmpty();
  });
}

test("missing local assets show an actionable error", async ({ page }) => {
  await page.route("**/asset-pack/manifest.json", (route) =>
    route.fulfill({ status: 404 }),
  );
  await page.goto("/");
  await expect(page.getByRole("alert")).toContainText("local asset pack");
  await expect(page.locator("#connection")).toHaveText("Unavailable");
});

test("unavailable renderers stop loading with a visible error", async ({
  page,
}) => {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "gpu", { value: undefined });
    HTMLCanvasElement.prototype.getContext = () => null;
  });
  await page.goto("/");
  await expect(page.locator("#connection")).toHaveText("Unavailable");
  await expect(page.getByRole("alert")).toContainText(
    "Canvas 2D is unavailable",
  );
});
