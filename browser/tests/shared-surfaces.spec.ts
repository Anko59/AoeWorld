import { expect, test, type Page, type TestInfo } from "@playwright/test";
import { PNG } from "pngjs";
import { gameAssets, syntheticSurfaceColors } from "./game-assets.js";
import { activateSyntheticMap } from "./synthetic-map.js";
import {
  saveSelectedCapture,
  waitForValidSelection,
} from "./surface-evidence.js";

const colors = syntheticSurfaceColors;

test.beforeEach(async ({ request }) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
});

test("shared terrain textures, painter transitions, and selection render", async ({
  page,
}, testInfo) => {
  testInfo.annotations.push({
    type: "assets",
    description: await gameAssets(page),
  });
  const project = testInfo.project.name;
  if (project === "canvas") {
    await page.addInitScript(() => {
      Object.defineProperty(navigator, "gpu", { value: undefined });
    });
  }
  await page.goto("/");
  const renderer = await waitForRenderer(page, project);
  testInfo.annotations.push({ type: "renderer", description: renderer });
  await page.waitForTimeout(500);
  const canvas = page.locator("#scene");
  const fallback = PNG.sync.read(await canvas.screenshot());
  await activateSyntheticMap(page);
  await expect
    .poll(() => residentChunkCount(page), { timeout: 15_000 })
    .toBeGreaterThan(0);
  await nextRenderFrames(page, 3);

  const terrain = PNG.sync.read(await canvas.screenshot());
  expect(changedPixels(fallback, terrain)).toBeGreaterThan(100);
  const evidence =
    testInfo.annotations.find((annotation) => annotation.type === "assets")
      ?.description ?? "";
  const counts = countColors(terrain);
  if (evidence === "generated CI fixtures") {
    assertSyntheticSurfaces(counts, terrain);
  } else {
    assertPrivateTexture(fallback, terrain, testInfo, "post-residency");
  }
  expect(distinctColors(terrain)).toBeGreaterThan(12);

  const box = await canvas.boundingBox();
  if (!box) throw new Error("shared terrain canvas is missing");
  const unselectedPng = await page.screenshot();
  await canvas.click({ position: { x: box.width / 2, y: box.height / 2 } });
  await waitForValidSelection(page);
  await nextRenderFrames(page, 2);
  const selectedPng = await page.screenshot();
  const selected = PNG.sync.read(selectedPng);
  expect(changedPixels(PNG.sync.read(unselectedPng), selected)).toBeGreaterThan(
    10,
  );
  if (evidence === "generated CI fixtures") {
    const selectedCounts = countColors(selected);
    for (const name of Object.keys(counts) as (keyof typeof counts)[]) {
      expect(
        selectedCounts[name],
        `${name} remains textured after selection`,
      ).toBeGreaterThan(24);
    }
  } else {
    assertPrivateTexture(fallback, selected, testInfo, "post-selection");
  }
  const yellow = colorPixels(selected, colors.ring, 12);
  expect(yellow).toBeGreaterThan(24);
  expect(colorComponents(selected, colors.ring, 18)).toBeGreaterThan(8);
  const residency = await residentChunkCount(page);
  expect(residency).toBeGreaterThan(0);

  const capturePath = await saveSelectedCapture(
    page,
    selectedPng,
    project,
    renderer,
    evidence,
    "surface",
  );
  testInfo.annotations.push(
    { type: "canonical capture", description: capturePath },
    { type: "terrain residency", description: `${residency} chunks` },
    { type: "selection pixels", description: `${yellow} pixels` },
  );
});

async function nextRenderFrames(page: Page, count: number) {
  for (let index = 0; index < count; index += 1) {
    await page.evaluate(
      () =>
        new Promise((resolve) => requestAnimationFrame(() => resolve(null))),
    );
  }
}

async function waitForRenderer(page: Page, project: string): Promise<string> {
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-assets",
    "aoe2-local",
  );
  const expected =
    project === "webgpu"
      ? /^webgpu$/
      : project === "canvas"
        ? /^canvas2d$/
        : /^(webgpu|canvas2d)$/;
  const playground = page.locator("#playground");
  await expect(playground).toHaveAttribute("data-renderer", expected);
  const renderer = await playground.getAttribute("data-renderer");
  if (!renderer) throw new Error("renderer backend is missing");
  return renderer;
}

async function residentChunkCount(page: Page): Promise<number> {
  const text = (await page.locator("#terrain-cache").textContent()) ?? "";
  const status = (await page.locator("#connection").textContent()) ?? "";
  if (status.includes("failed")) throw new Error(status);
  return Number(/\((\d+) \/ 512 chunks\)/.exec(text)?.[1] ?? 0);
}

function assertSyntheticSurfaces(
  counts: Record<keyof typeof colors, number>,
  image: PNG,
) {
  const pairs: ReadonlyArray<
    readonly [keyof typeof colors, keyof typeof colors]
  > = [
    ["grass", "grassAccent"],
    ["ramp", "rampAccent"],
    ["cliff", "cliffAccent"],
    ["water", "waterAccent"],
    ["shore", "shoreAccent"],
  ];
  for (const [base, accent] of pairs) {
    expect(counts[base], `${base} textured surface pixels`).toBeGreaterThan(64);
    expect(counts[accent], `${base} atlas accent pixels`).toBeGreaterThan(32);
    expect(
      adjacentPixels(image, colors[base], colors[accent]),
      `${base} texture adjacency`,
    ).toBeGreaterThan(16);
  }
  expect(counts.skirt, "cliff skirt pixels").toBeGreaterThan(24);
  expect(adjacentPixels(image, colors.cliff, colors.skirt)).toBeGreaterThan(8);
  expect(colorBandsTouch(image, colors.water, colors.shore)).toBe(true);
}

function assertPrivateTexture(
  fallback: PNG,
  image: PNG,
  testInfo: TestInfo,
  phase: "post-residency" | "post-selection",
) {
  const texture = changedTextureVariation(fallback, image);
  expect(texture.changed).toBeGreaterThan(1_000);
  expect(texture.colors).toBeGreaterThan(16);
  expect(texture.transitions).toBeGreaterThan(128);
  testInfo.annotations.push({
    type: `private terrain texture ${phase}`,
    description: `${texture.colors} colors, ${texture.transitions} transitions`,
  });
}

function changedTextureVariation(first: PNG, second: PNG) {
  expect(second.width).toBe(first.width);
  expect(second.height).toBe(first.height);
  const statusTop = Math.floor(first.height * 0.86);
  const changed = new Uint8Array(first.width * first.height);
  let changedPixels = 0;
  const colors = new Set<string>();
  for (let y = 0; y < statusTop; y += 1) {
    for (let x = 0; x < first.width; x += 1) {
      const pixel = y * first.width + x;
      const offset = pixel * 4;
      if (
        first.data[offset] === second.data[offset] &&
        first.data[offset + 1] === second.data[offset + 1] &&
        first.data[offset + 2] === second.data[offset + 2]
      ) {
        continue;
      }
      changed[pixel] = 1;
      changedPixels += 1;
      colors.add(
        `${second.data[offset]},${second.data[offset + 1]},${second.data[offset + 2]}`,
      );
    }
  }
  let transitions = 0;
  for (let y = 0; y < statusTop; y += 1) {
    for (let x = 0; x < first.width; x += 1) {
      const pixel = y * first.width + x;
      if (!changed[pixel]) continue;
      for (const neighbor of [pixel + 1, pixel + first.width]) {
        const outside =
          neighbor >= changed.length ||
          (neighbor % first.width === 0 && neighbor === pixel + 1);
        if (outside || !changed[neighbor]) continue;
        if (differentColors(second, pixel, neighbor)) transitions += 1;
      }
    }
  }
  return { changed: changedPixels, colors: colors.size, transitions };
}

function differentColors(image: PNG, first: number, second: number) {
  const left = first * 4;
  const right = second * 4;
  return (
    image.data[left] !== image.data[right] ||
    image.data[left + 1] !== image.data[right + 1] ||
    image.data[left + 2] !== image.data[right + 2]
  );
}

function countColors(image: PNG): Record<keyof typeof colors, number> {
  return Object.fromEntries(
    Object.entries(colors).map(([name, color]) => [
      name,
      colorPixels(image, color, 6),
    ]),
  ) as Record<keyof typeof colors, number>;
}

function changedPixels(first: PNG, second: PNG) {
  expect(second.width).toBe(first.width);
  expect(second.height).toBe(first.height);
  let changed = 0;
  for (let offset = 0; offset < first.data.length; offset += 4) {
    if (
      first.data[offset] !== second.data[offset] ||
      first.data[offset + 1] !== second.data[offset + 1] ||
      first.data[offset + 2] !== second.data[offset + 2]
    )
      changed += 1;
  }
  return changed;
}

function distinctColors(image: PNG) {
  const colors = new Set<string>();
  for (let offset = 0; offset < image.data.length; offset += 4) {
    colors.add(
      `${image.data[offset]},${image.data[offset + 1]},${image.data[offset + 2]}`,
    );
  }
  return colors.size;
}

function colorPixels(
  image: PNG,
  expected: readonly number[],
  tolerance: number,
) {
  let count = 0;
  for (let offset = 0; offset < image.data.length; offset += 4) {
    if (matches(image.data, offset, expected, tolerance)) count += 1;
  }
  return count;
}

function adjacentPixels(
  image: PNG,
  first: readonly number[],
  second: readonly number[],
) {
  let count = 0;
  for (let y = 0; y < image.height; y += 1) {
    for (let x = 0; x < image.width; x += 1) {
      const offset = (y * image.width + x) * 4;
      if (x + 1 < image.width) {
        const right = offset + 4;
        if (
          (matches(image.data, offset, first, 6) &&
            matches(image.data, right, second, 6)) ||
          (matches(image.data, offset, second, 6) &&
            matches(image.data, right, first, 6))
        )
          count += 1;
      }
      if (y + 1 < image.height) {
        const down = offset + image.width * 4;
        if (
          (matches(image.data, offset, first, 6) &&
            matches(image.data, down, second, 6)) ||
          (matches(image.data, offset, second, 6) &&
            matches(image.data, down, first, 6))
        )
          count += 1;
      }
    }
  }
  return count;
}

function colorBandsTouch(
  image: PNG,
  first: readonly number[],
  second: readonly number[],
) {
  const left = colorBounds(image, first);
  const right = colorBounds(image, second);
  const horizontal = left.maxX + 8 >= right.minX && right.maxX + 8 >= left.minX;
  const vertical = left.maxY + 16 >= right.minY && right.maxY + 16 >= left.minY;
  return horizontal && vertical;
}

function colorBounds(image: PNG, expected: readonly number[]) {
  let minX = image.width;
  let minY = image.height;
  let maxX = -1;
  let maxY = -1;
  for (let y = 0; y < image.height; y += 1) {
    for (let x = 0; x < image.width; x += 1) {
      const offset = (y * image.width + x) * 4;
      if (!matches(image.data, offset, expected, 6)) continue;
      minX = Math.min(minX, x);
      minY = Math.min(minY, y);
      maxX = Math.max(maxX, x);
      maxY = Math.max(maxY, y);
    }
  }
  return { minX, minY, maxX, maxY };
}

function colorComponents(
  image: PNG,
  expected: readonly number[],
  tolerance: number,
) {
  const seen = new Uint8Array(image.width * image.height);
  let components = 0;
  for (let y = 0; y < image.height; y += 1) {
    for (let x = 0; x < image.width; x += 1) {
      const index = y * image.width + x;
      if (seen[index]) continue;
      const offset = index * 4;
      if (!matches(image.data, offset, expected, tolerance)) continue;
      components += 1;
      const pending = [index];
      seen[index] = 1;
      while (pending.length) {
        const current = pending.pop() as number;
        const cx = current % image.width;
        const cy = Math.floor(current / image.width);
        for (const [dx, dy] of [
          [1, 0],
          [-1, 0],
          [0, 1],
          [0, -1],
        ] as const) {
          const nx = cx + dx;
          const ny = cy + dy;
          if (nx < 0 || ny < 0 || nx >= image.width || ny >= image.height)
            continue;
          const next = ny * image.width + nx;
          if (seen[next]) continue;
          if (!matches(image.data, next * 4, expected, tolerance)) continue;
          seen[next] = 1;
          pending.push(next);
        }
      }
    }
  }
  return components;
}

function matches(
  data: Uint8Array,
  offset: number,
  expected: readonly number[],
  tolerance: number,
) {
  return expected.every(
    (value, index) =>
      Math.abs((data[offset + index] ?? 0) - value) <= tolerance,
  );
}
