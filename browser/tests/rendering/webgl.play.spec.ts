import { expect, test, type Page } from "@playwright/test";
import { PNG } from "pngjs";
import { gameAssets, syntheticSurfaceColors } from "../game-assets.js";
import { activateSyntheticMap } from "../synthetic-map.js";
import { forceCanvas, forceWebGl } from "./backends.js";

const colors = Object.values(syntheticSurfaceColors);

test.beforeEach(async ({ request }) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
});

async function ready(page: Page, backend: "webgl2" | "canvas2d") {
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-assets",
    "aoe2-local",
  );
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    backend,
  );
  await expect(page.getByRole("alert")).toBeEmpty();
}

function paletteClass(image: PNG, offset: number): number {
  // Cliff and skirt differ by only [6, 5, 6], so both match tolerance 6.
  // First-match classification steals every skirt pixel for the cliff class.
  // Keep that tolerance, but resolve overlapping candidates by nearest RGB.
  let selected = -1;
  let minimum = Number.POSITIVE_INFINITY;
  for (let index = 0; index < colors.length; index++) {
    const color = colors[index];
    if (!color) continue;
    let distance = 0;
    for (let axis = 0; axis < 3; axis++) {
      const delta = (image.data[offset + axis] ?? 0) - (color[axis] ?? 0);
      if (Math.abs(delta) > 6) {
        distance = Number.POSITIVE_INFINITY;
        break;
      }
      distance += delta * delta;
    }
    if (distance < minimum) {
      minimum = distance;
      selected = index;
    }
  }
  return selected;
}

function paletteCounts(image: PNG): number[] {
  const counts = colors.map(() => 0);
  for (let offset = 0; offset < image.data.length; offset += 4) {
    const index = paletteClass(image, offset);
    if (index >= 0) counts[index] = (counts[index] ?? 0) + 1;
  }
  return counts;
}

async function selectedTerrain(page: Page, backend: "webgl2" | "canvas2d") {
  await gameAssets(page, true);
  await (backend === "webgl2" ? forceWebGl(page) : forceCanvas(page));
  await page.goto("/");
  await ready(page, backend);
  await activateSyntheticMap(page);
  const canvas = page.locator("#scene");
  const box = await canvas.boundingBox();
  if (!box) throw new Error("Missing game canvas");
  await canvas.click({ position: { x: box.width / 2, y: box.height / 2 } });
  await expect(page.locator("#unit-state")).toContainText("unit");
  await expect(page.locator("#unit-state")).not.toHaveText("no unit selected");
  await expect
    .poll(
      async () =>
        paletteCounts(PNG.sync.read(await canvas.screenshot())).at(-1) ?? 0,
    )
    .toBeGreaterThan(24);
  return PNG.sync.read(await canvas.screenshot());
}

test("WebGL2 draws textured slopes, cliffs, skirts, water and selection pixels", async ({
  page,
}) => {
  const image = await selectedTerrain(page, "webgl2");
  const counts = paletteCounts(image);
  for (let index = 0; index < colors.length; index++) {
    expect(counts[index], `surface palette ${index}`).toBeGreaterThan(
      index === colors.length - 1 ? 24 : 32,
    );
  }
});

test("WebGL2 depth and material masks agree with actual software Canvas pixels", async ({
  page,
  browser,
}) => {
  const gpu = await selectedTerrain(page, "webgl2");
  const referencePage = await browser.newPage({
    viewport: page.viewportSize() ?? { width: 1280, height: 720 },
  });
  try {
    const reference = await selectedTerrain(referencePage, "canvas2d");
    expect(gpu.width).toBe(reference.width);
    expect(gpu.height).toBe(reference.height);
    // Compare world pixels, not renderer badges, minimap markers, or HUD text.
    // A one-pixel raster edge can differ between f32 hardware and f64 software.
    let classified = 0;
    let matching = 0;
    const expectedByClass = colors.map(() => 0);
    const matchingByClass = colors.map(() => 0);
    for (let y = 100; y < gpu.height - 120; y++) {
      for (let x = 32; x < gpu.width - 280; x++) {
        const offset = (y * gpu.width + x) * 4;
        const expected = paletteClass(reference, offset);
        if (expected < 0) continue;
        classified++;
        expectedByClass[expected] = (expectedByClass[expected] ?? 0) + 1;
        if (paletteClass(gpu, offset) === expected) {
          matching++;
          matchingByClass[expected] = (matchingByClass[expected] ?? 0) + 1;
        }
      }
    }
    expect(classified).toBeGreaterThan(10_000);
    expect(matching / classified).toBeGreaterThan(0.95);
    // Grass must not hide a broken small cliff/water/depth region in the total.
    // Selection is independently pixel-checked above; GPU edge coverage differs.
    for (let index = 0; index < colors.length - 1; index++) {
      const count = expectedByClass[index] ?? 0;
      expect(count, `reference terrain class ${index}`).toBeGreaterThan(32);
      expect(
        (matchingByClass[index] ?? 0) / count,
        `terrain/depth class ${index}`,
      ).toBeGreaterThan(0.9);
    }
    // The reference really denies GL, rather than accidentally testing it twice.
    expect(
      await referencePage.evaluate(() => {
        const probe = document.createElement("canvas");
        return (
          !Reflect.get(navigator, "gpu") &&
          probe.getContext("webgl2") === null &&
          probe.getContext("2d") !== null
        );
      }),
    ).toBe(true);
  } finally {
    await referencePage.close();
  }
});

async function horsePixels(
  page: Page,
): Promise<{ x: number; y: number; pixels: number }> {
  const image = PNG.sync.read(await page.locator("#scene").screenshot());
  let pixels = 0,
    sumX = 0,
    sumY = 0;
  for (
    let y = Math.floor(image.height / 2) - 150;
    y < Math.floor(image.height / 2) + 150;
    y++
  ) {
    for (
      let x = Math.floor(image.width / 2) - 220;
      x < Math.floor(image.width / 2) + 220;
      x++
    ) {
      const offset = (y * image.width + x) * 4;
      const [r = 0, g = 0, b = 0] = image.data.subarray(offset, offset + 3);
      if (b > 150 && b > r + 30 && b > g + 30) {
        pixels++;
        sumX += x;
        sumY += y;
      }
    }
  }
  return {
    x: sumX / Math.max(1, pixels),
    y: sumY / Math.max(1, pixels),
    pixels,
  };
}

async function selectAndMove(page: Page) {
  const canvas = page.locator("#scene");
  const box = await canvas.boundingBox();
  if (!box) throw new Error("Missing game canvas");
  await expect
    .poll(async () => (await horsePixels(page)).pixels)
    .toBeGreaterThan(10);
  const before = await horsePixels(page);
  await canvas.click({ position: { x: box.width / 2, y: box.height / 2 } });
  await canvas.click({
    position: { x: box.width / 2 + 110, y: box.height / 2 + 30 },
    button: "right",
  });
  await expect(page.locator("#unit-state")).toContainText("moving");
  await expect
    .poll(async () => {
      const after = await horsePixels(page);
      expect(after.pixels).toBeGreaterThan(10);
      return Math.hypot(after.x - before.x, after.y - before.y);
    })
    .toBeGreaterThan(3);
}

test("WebGL2 displays authoritative scout movement and keyboard camera changes", async ({
  page,
}) => {
  await gameAssets(page, true);
  await forceWebGl(page);
  await page.goto("/");
  await ready(page, "webgl2");
  await selectAndMove(page);
  const origin = await page.locator("#world-position").innerText();
  await page.keyboard.press("ArrowRight");
  await expect(page.locator("#world-position")).not.toHaveText(origin);
  await expect(page.getByRole("alert")).toBeEmpty();
});

async function loseAndRestore(page: Page) {
  const canvas = await page.locator("#scene").elementHandle();
  if (!canvas) throw new Error("Missing WebGL canvas");
  const extension = await canvas.evaluateHandle((element) => {
    const node = element as HTMLCanvasElement;
    const gl = node.getContext("webgl2");
    if (!gl) throw new Error("Actual WebGL2 context is required");
    const extension = gl.getExtension("WEBGL_lose_context");
    if (!extension)
      throw new Error("Pinned Chromium must support WEBGL_lose_context");
    Reflect.set(window, "__webglLost", false);
    Reflect.set(window, "__webglRestored", false);
    Reflect.set(window, "__rendererRestored", false);
    node.addEventListener(
      "webglcontextlost",
      () => Reflect.set(window, "__webglLost", true),
      { once: true },
    );
    node.addEventListener(
      "webglcontextrestored",
      () => Reflect.set(window, "__webglRestored", true),
      { once: true },
    );
    node.addEventListener(
      "aoe-renderer-restored",
      () => Reflect.set(window, "__rendererRestored", true),
      { once: true },
    );
    return extension;
  });
  try {
    await extension.evaluate((extension) => extension.loseContext());
    await expect
      .poll(() => page.evaluate(() => Reflect.get(window, "__webglLost")))
      .toBe(true);
    expect(
      await canvas.evaluate((element) =>
        (element as HTMLCanvasElement).getContext("webgl2")?.isContextLost(),
      ),
    ).toBe(true);
    await extension.evaluate((extension) => extension.restoreContext());
    await expect
      .poll(() => page.evaluate(() => Reflect.get(window, "__webglRestored")))
      .toBe(true);
    await expect
      .poll(() =>
        page.evaluate(() => Reflect.get(window, "__rendererRestored")),
      )
      .toBe(true);
    expect(
      await canvas.evaluate(
        (element) =>
          element === document.getElementById("scene") && element.isConnected,
      ),
    ).toBe(true);
    expect(
      await canvas.evaluate((element) =>
        (element as HTMLCanvasElement).getContext("webgl2")?.isContextLost(),
      ),
    ).toBe(false);
    await ready(page, "webgl2");
  } finally {
    await extension.dispose();
    await canvas.dispose();
  }
}

test("WebGL2 restores textured idle-world pixels without camera or unit cache invalidation", async ({
  page,
}) => {
  const before = await selectedTerrain(page, "webgl2");
  const expected = paletteCounts(before);
  const origin = await page.locator("#world-position").innerText();
  await loseAndRestore(page);
  // No camera/selection/resize input here: the restoration notification alone
  // must invalidate the idle render cache and reupload the retained atlas.
  await expect
    .poll(async () => {
      const restored = paletteCounts(
        PNG.sync.read(await page.locator("#scene").screenshot()),
      );
      return Math.min(
        ...expected.map(
          (count, index) => (restored[index] ?? 0) / Math.max(1, count),
        ),
      );
    })
    .toBeGreaterThan(0.95);
  await expect(page.locator("#world-position")).toHaveText(origin);
  await expect(page.getByRole("alert")).toBeEmpty();
});

test("WebGL2 context restoration retains scout input, moving pixels and camera controls", async ({
  page,
}) => {
  await gameAssets(page, true);
  await forceWebGl(page);
  await page.goto("/");
  await ready(page, "webgl2");
  await expect
    .poll(async () => (await horsePixels(page)).pixels)
    .toBeGreaterThan(10);
  await loseAndRestore(page);
  await selectAndMove(page);
  const origin = await page.locator("#world-position").innerText();
  await page.keyboard.press("ArrowRight");
  await expect(page.locator("#world-position")).not.toHaveText(origin);
  await expect(page.getByRole("alert")).toBeEmpty();
});

test("bound WebGL2 initialization failure replaces its canvas with playable Canvas 2D", async ({
  page,
}) => {
  await gameAssets(page, true);
  await forceWebGl(page);
  await page.addInitScript(() => {
    const original = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = function (
      this: HTMLCanvasElement,
      type: string,
      ...args: unknown[]
    ) {
      const result = Reflect.apply(original, this, [type, ...args]);
      if (type === "webgl2" && result) {
        Reflect.set(window, "__boundWebGlFailure", true);
        const gl = result as WebGL2RenderingContext;
        const parameter = gl.getParameter.bind(gl);
        gl.getParameter = (name: number) =>
          name === gl.DEPTH_BITS ? 16 : parameter(name);
      }
      return result;
    } as typeof original;
  });
  await page.goto("/");
  await ready(page, "canvas2d");
  expect(
    await page.evaluate(() => Reflect.get(window, "__boundWebGlFailure")),
  ).toBe(true);
  await selectAndMove(page);
  expect(
    await page
      .locator("#scene")
      .evaluate(
        (element) => (element as HTMLCanvasElement).getContext("2d") !== null,
      ),
  ).toBe(true);
});
