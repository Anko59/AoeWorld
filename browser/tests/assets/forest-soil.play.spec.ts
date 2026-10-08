import { expect, test, type Page } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { join } from "node:path";
import { PNG } from "pngjs";
import { forceCanvas, forceWebGl } from "../rendering/backends.js";

const ORIGINAL =
  "7e6fa0da194d13fcff0cd50e74fe92215fd11b5bcce4556e4dad3d34ff7447ce";
type Frame = {
  source: string;
  frame: number;
  page: number;
  x: number;
  y: number;
  width: number;
  height: number;
};
type Manifest = {
  version: number;
  input_hash: string;
  frames: Frame[];
  pages: { width: number; height: number }[];
};
type Point = [number, number];

function required(name: string): string {
  const value = process.env[name];
  if (!value)
    throw new Error(`${name} is required for explicit original/source proof`);
  return value;
}

async function camera(page: Page): Promise<Point> {
  const text = await page.locator("#world-position").innerText();
  const match = /^(-?\d+(?:\.\d+)?),\s*(-?\d+(?:\.\d+)?)$/.exec(text);
  if (!match) throw new Error(`Unrecognized live camera: ${text}`);
  return [Number(match[1]), Number(match[2])];
}

async function ready(page: Page, backend: string) {
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-assets",
    "aoe2-local",
    { timeout: 60_000 },
  );
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    backend,
    { timeout: 60_000 },
  );
}

async function jump(page: Page, target: Point) {
  const minimap = page.locator("#minimap");
  const width = Number(await minimap.getAttribute("data-world-width"));
  const height = Number(await minimap.getAttribute("data-world-height"));
  expect(width).toBeGreaterThan(target[0]);
  expect(height).toBeGreaterThan(target[1]);
  await page.getByRole("button", { name: "Show whole map" }).click();
  const box = await page.locator("#minimap-map").boundingBox();
  if (!box) throw new Error("Missing real whole-map navigation control");
  const side = Math.min(box.width, box.height);
  await page.mouse.click(
    box.x + (box.width - side) / 2 + (side * (target[0] + 0.5)) / width,
    box.y + (box.height - side) / 2 + (side * (target[1] + 0.5)) / height,
  );
  await page.getByRole("button", { name: "Close map overview" }).click();
  await expect
    .poll(
      async () => {
        const point = await camera(page);
        return Math.max(
          Math.abs(point[0] - target[0] - 0.5),
          Math.abs(point[1] - target[1] - 0.5),
        );
      },
      { timeout: 30_000 },
    )
    .toBeLessThan(2);
}

async function settle(page: Page) {
  await page.waitForLoadState("networkidle", { timeout: 60_000 });
  await page.evaluate(
    () =>
      new Promise<void>((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
      ),
  );
  const label = await page.locator("#terrain-cache").innerText();
  const match =
    /terrain cache: ([\d.]+) \/ 128 MiB \((\d+) \/ 512 chunks\)/.exec(label);
  if (!match) throw new Error(`Unrecognized live residency: ${label}`);
  const residentMiB = Number(match[1]);
  const residentChunks = Number(match[2]);
  expect(residentMiB).toBeLessThanOrEqual(128);
  expect(residentChunks).toBeGreaterThan(0);
  expect(residentChunks).toBeLessThanOrEqual(512);
  return { label, residentMiB, residentChunks };
}

// Observe scale via real middle-drag input and the shipping camera readout.
// The readout rounds to 0.1 tile; this is an estimate, not a hidden zoom export.
async function scaleObservation(page: Page) {
  const scene = page.locator("#scene");
  const box = await scene.boundingBox();
  if (!box) throw new Error("Missing live scene");
  const before = await camera(page);
  const start = [box.x + box.width / 2, box.y + box.height / 2] as const;
  const pixels = Math.min(400, box.width / 3);
  await page.mouse.move(...start);
  await page.mouse.down({ button: "middle" });
  await page.mouse.move(start[0] + pixels, start[1], { steps: 4 });
  await page.mouse.up({ button: "middle" });
  await expect
    .poll(async () => JSON.stringify(await camera(page)))
    .not.toBe(JSON.stringify(before));
  const after = await camera(page);
  const difference = Math.abs(after[0] - before[0] - (after[1] - before[1]));
  expect(difference).toBeGreaterThan(0.2);
  return {
    estimatedZoom: pixels / (64 * difference),
    readoutPrecisionTiles: 0.1,
    measurement:
      "horizontal drag / (ISO half-width 64 * camera axis-difference)",
    dragCssPixels: pixels,
    before,
    after,
  };
}

async function inspectTarget(page: Page, target: Point) {
  const scene = page.locator("#scene");
  const box = await scene.boundingBox();
  if (!box) throw new Error("Missing scene for live terrain inspection");
  // Height-aware picking is shipping behavior. Search a bounded central region;
  // do not override the camera focus or forge a terrain-inspection label.
  for (const yOffset of [
    0, -32, 32, -64, 64, -128, 128, -192, 192, -256, 256,
  ]) {
    for (const xOffset of [0, -32, 32, -64, 64]) {
      const x = box.width / 2 + xOffset;
      const y = box.height / 2 + yOffset;
      if (x < 1 || y < 1 || x >= box.width - 1 || y >= box.height - 1) continue;
      await scene.hover({ position: { x, y } });
      await page.evaluate(
        () =>
          new Promise<void>((resolve) =>
            requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
          ),
      );
      const text = await page.locator("#tile-inspection").innerText();
      if (
        text.startsWith(`tile ${target[0]}, ${target[1]}:`) &&
        text.includes("ForestFloor")
      )
        return { text, pointerCss: [x, y] };
    }
  }
  throw new Error(
    `Supplied source ForestFloor target is not visibly inspectable: ${target}`,
  );
}

function targetEvidence(
  chunk: { x: number; y: number; payload_hex: string },
  target: Point,
) {
  expect(chunk.x).toBe(Math.floor(target[0] / 32));
  expect(chunk.y).toBe(Math.floor(target[1] / 32));
  expect(chunk.payload_hex).toMatch(/^[0-9a-f]+$/);
  expect(chunk.payload_hex.length).toBeLessThanOrEqual(2_097_152);
  const bytes = Buffer.from(chunk.payload_hex, "hex");
  expect(bytes[0]).toBe(3);
  const count = bytes.readUInt16LE(1);
  expect(count).toBeGreaterThan(0);
  expect(count).toBeLessThanOrEqual(1024);
  const resourceCount = bytes.readUInt16LE(3);
  expect(resourceCount).toBeLessThanOrEqual(1024);
  const resourceOffset = 7 + count * 37;
  if (bytes.length < resourceOffset + resourceCount * 22)
    throw new Error("Truncated real codec3 terrain/resources");
  const woodFamilies: Record<string, { count: number; variants: number[] }> =
    {};
  for (let index = 0; index < resourceCount; index++) {
    const offset = resourceOffset + index * 22;
    if (bytes[offset + 16] !== 1) continue; // Published ResourceKind::Wood.
    const family = String(bytes[offset + 21]);
    const entry = woodFamilies[family] ?? { count: 0, variants: [] };
    entry.count++;
    const variant = bytes[offset + 20];
    if (variant === undefined)
      throw new Error("Missing source resource variant");
    if (!entry.variants.includes(variant)) entry.variants.push(variant);
    woodFamilies[family] = entry;
  }
  for (let index = 0; index < count; index++) {
    const offset = 7 + index * 37;
    if (
      bytes.readInt32LE(offset + 20) !== target[0] ||
      bytes.readInt32LE(offset + 24) !== target[1]
    )
      continue;
    const material = bytes.readUInt32LE(offset + 14) & 15;
    expect(material).toBe(3); // Published wire GroundMaterial::ForestFloor, not scene slot6.
    expect(bytes[offset + 28]).toBe(1);
    const canopy = bytes.readUInt16LE(offset + 29);
    const floor = bytes.readUInt16LE(offset + 31);
    expect(canopy).toBeGreaterThan(0);
    expect(canopy).toBeLessThanOrEqual(1000);
    expect(floor).toBeGreaterThan(0);
    expect(floor).toBeLessThanOrEqual(1000);
    return {
      codec: 3,
      material: "ForestFloor",
      canopy,
      floor,
      target,
      resourceCount,
      woodFamilies,
      geographicHeightCentimeters: bytes.readInt32LE(offset),
      movementLevel: bytes.readInt16LE(offset + 4),
      palette: bytes[offset + 33],
      exposure: bytes[offset + 34],
      heightBand: bytes[offset + 35],
    };
  }
  throw new Error("Supplied target absent from real source chunk");
}

test("explicit original forest soil appears on actual source terrain across three backends and zooms", async ({
  browser,
  request,
}) => {
  test.skip(
    process.env["AOE_FOREST_SOIL_PROOF"] !== "1",
    "Explicit private original/source qualification only",
  );
  test.setTimeout(600_000);
  const output = required("AOE_FOREST_SOIL_OUTPUT");
  const hash = required("AOE_COUNTRY_SOURCE_HASH");
  expect(hash).toMatch(/^[0-9a-f]{64}$/);
  const coordinates = /^([0-9]+),([0-9]+)$/.exec(
    required("AOE_FOREST_SOIL_TARGET_TILE"),
  );
  if (!coordinates)
    throw new Error(
      "AOE_FOREST_SOIL_TARGET_TILE must be nonnegative integer x,y",
    );
  const target: Point = [Number(coordinates[1]), Number(coordinates[2])];
  for (const value of target)
    expect(Number.isSafeInteger(value) && value < 262144).toBe(true);
  await mkdir(output, { recursive: true });
  const manifestResponse = await request.get("/asset-pack/manifest.json");
  expect(manifestResponse.ok()).toBe(true);
  const manifest: Manifest = await manifestResponse.json();
  expect(manifest.version).toBe(1);
  expect(manifest.input_hash).toBe(ORIGINAL);
  expect(manifest.frames.length).toBeLessThanOrEqual(25_000);
  expect(manifest.pages.length).toBeGreaterThan(0);
  expect(manifest.pages.length).toBeLessThanOrEqual(256);
  const prefix = manifest.frames
    .filter(
      (frame) =>
        frame.source === "terrain.drs:[32, 112, 108, 115]:15011" &&
        frame.frame < 10,
    )
    .sort((a, b) => a.frame - b.frame);
  expect(prefix).toHaveLength(10);
  prefix.forEach((frame, index) => {
    expect(frame.frame).toBe(index);
    const page = manifest.pages[frame.page];
    if (!page) throw new Error("Forest prefix has invalid source page");
    expect(frame.width).toBeGreaterThan(0);
    expect(frame.height).toBeGreaterThan(0);
    expect(frame.x).toBeGreaterThanOrEqual(0);
    expect(frame.y).toBeGreaterThanOrEqual(0);
    expect(frame.x + frame.width).toBeLessThanOrEqual(page.width);
    expect(frame.y + frame.height).toBeLessThanOrEqual(page.height);
  });
  let resume: { origin: string; token: string } | null = null;
  const observations: unknown[] = [];
  for (const backend of ["webgpu", "webgl2", "canvas2d"] as const) {
    const page = await browser.newPage({
      viewport: { width: 1280, height: 720 },
    });
    const errors: string[] = [];
    const fetched = new Set<string>();
    try {
      if (resume)
        await page.addInitScript(({ origin, token }) => {
          if (location.origin === origin)
            sessionStorage.setItem("aoeworld.resume-token", token);
        }, resume);
      if (backend === "webgl2") await forceWebGl(page);
      if (backend === "canvas2d") await forceCanvas(page);
      page.on("pageerror", (error) => errors.push(error.message));
      page.on("console", (message) => {
        if (message.type() === "error") errors.push(message.text());
      });
      page.on("requestfailed", (failure) =>
        errors.push(`${failure.url()}: ${failure.failure()?.errorText}`),
      );
      page.on("response", (response) => {
        const path = new URL(response.url()).pathname;
        if (path.startsWith(`/maps/${hash}/chunks/`)) {
          if (!response.ok()) errors.push(`${path}: HTTP ${response.status()}`);
          else fetched.add(path);
        }
      });
      await page.goto("/");
      await ready(page, backend);
      // Inspect an already-active package without taking the user's controller
      // or resetting their world; activation remains authenticated when needed.
      const activeHash = await page
        .locator("#minimap")
        .getAttribute("data-map-hash");
      if (!resume && activeHash !== hash) {
        await page.getByRole("button", { name: "Open map creator" }).click();
        await page.getByLabel("Saved maps").selectOption(hash);
        await page.getByRole("button", { name: "Open saved" }).click();
        await expect(page.locator("#map-estimate")).toContainText(
          `Saved map active: ${hash.slice(0, 12)}`,
          { timeout: 60_000 },
        );
        await expect(page.locator("#connection")).toHaveText(/reconnect/);
        await page.getByRole("button", { name: "Reconnect to server" }).click();
        await ready(page, backend);
      }
      await expect(page.locator("#minimap")).toHaveAttribute(
        "data-map-hash",
        hash,
        { timeout: 60_000 },
      );
      await page
        .getByRole("button", { name: "Center on primary unit" })
        .click();
      await jump(page, target);
      await settle(page);
      const chunkPath = `/maps/${hash}/chunks/${Math.floor(target[0] / 32)}/${Math.floor(target[1] / 32)}`;
      await expect
        .poll(() => fetched.has(chunkPath), { timeout: 60_000 })
        .toBe(true);
      const chunkResponse = await page.request.get(chunkPath);
      expect(chunkResponse.ok()).toBe(true);
      const chunk = await chunkResponse.json();
      const source = targetEvidence(chunk, target);
      const captures: unknown[] = [];
      for (const [index, wheelDelta] of [0, -200, 400].entries()) {
        if (wheelDelta !== 0) {
          await page.locator("#scene").hover();
          await page.mouse.wheel(0, wheelDelta);
        }
        const zoom = await scaleObservation(page);
        await jump(page, target);
        const cache = await settle(page);
        const inspection = await inspectTarget(page, target);
        const image = await page.locator("#scene").screenshot();
        const png = PNG.sync.read(image);
        expect(png.width * png.height).toBeLessThanOrEqual(4_194_304);
        const colors = new Set<number>();
        for (let pixel = 0; pixel < png.data.length; pixel += 4)
          colors.add(
            ((png.data[pixel] ?? 0) << 16) |
              ((png.data[pixel + 1] ?? 0) << 8) |
              (png.data[pixel + 2] ?? 0),
          );
        expect(colors.size).toBeGreaterThan(50);
        const file = `${backend}-forest-${index}.png`;
        await writeFile(join(output, file), image);
        captures.push({
          file,
          wheelDelta,
          zoom,
          camera: await camera(page),
          inspection,
          cache,
          pixels: {
            width: png.width,
            height: png.height,
            distinctColors: colors.size,
          },
        });
      }
      const healthResponse = await page.request.get("/health");
      expect(healthResponse.ok()).toBe(true);
      const health = await healthResponse.json();
      const token = await page.evaluate(() =>
        sessionStorage.getItem("aoeworld.resume-token"),
      );
      if (token) resume = { origin: new URL(page.url()).origin, token };
      expect(fetched.size).toBeLessThanOrEqual(1024);
      expect(errors).toEqual([]);
      observations.push({
        backend,
        packageHash: hash,
        target,
        source,
        chunkPath,
        health,
        clientWasm: await (async () => {
          // Server build health does not identify a bind-mounted working client.
          const urls = await page.evaluate(() =>
            performance
              .getEntriesByType("resource")
              .map((entry) => entry.name)
              .filter((name) => /\.wasm(?:\?|$)/.test(name)),
          );
          expect(urls).toHaveLength(1);
          const url = urls[0];
          if (!url || new URL(url).origin !== new URL(page.url()).origin)
            throw new Error("Missing same-origin served WASM");
          const response = await page.request.get(url);
          expect(response.ok()).toBe(true);
          const body = await response.body();
          expect(body.length).toBeLessThanOrEqual(16_777_216);
          return {
            url,
            bytes: body.length,
            sha256: createHash("sha256").update(body).digest("hex"),
          };
        })(),
        captures,
        fetchedChunks: [...fetched].sort(),
        errors,
      });
    } finally {
      await page.close();
    }
  }
  await writeFile(
    join(output, "forest-soil.json"),
    JSON.stringify(
      {
        originalManifest: {
          version: manifest.version,
          inputHash: manifest.input_hash,
          reviewedPrefix: prefix,
        },
        observations,
        limits:
          "Real supplied source ForestFloor interior only; no synthetic chunks or shipping test exports. Ten reviewed accents are not approval of a 100-frame periodic sheet. Zoom estimates use rounded UI camera readouts. Captures do not prove fixed zoom/LOD tiling, edge blending, ecosystem accuracy, eviction or dedicated hardware performance.",
      },
      null,
      2,
    ),
  );
});
