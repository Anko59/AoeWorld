import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { expect, type Page } from "@playwright/test";
import { PNG } from "pngjs";
import { gameAssets } from "./game-assets.js";
import { captureEvictionCase } from "./source-geographic-eviction-capture.js";
import type { EvictionTarget } from "./source-geographic-eviction.js";

type SourceLock = {
  id: string;
  provider: string;
  release: string;
  url: string;
  sha256: number[];
  native_resolution: string;
  crs: string;
  vertical_datum: string;
  license: string;
  preprocessing_version: string;
};

export type VisualCase = {
  id: string;
  geometry: "inland" | "ocean" | "relief";
  content_hash: string;
  manifest_path: string;
  location: string;
  request: Record<string, unknown>;
  tiles_per_side: number;
  package_chunk_count_bound: number;
  schema_version: number;
  generator_version: number;
  generation_recipe_version: number;
  preparation_elapsed_milliseconds: number;
  page_count: number;
  page_bytes: number;
  historical_land_use: {
    level_zero_pages: number;
    coverage_present_pages: number;
    legacy_coverage_missing_pages: number;
    coverage_samples: number;
    land_percent_sum: number;
    valid_land_percent_sum: number;
    lake_percent_sum: number;
    ocean_percent_sum: number;
    nodata_percent_sum: number;
    outside_percent_sum: number;
  };
  source_locks: SourceLock[];
  water_pages: {
    sample_count: number;
    ocean_nonzero_samples: number;
    inland_nonzero_samples: number;
    ocean_coverage_percent_sum: number;
    inland_coverage_percent_sum: number;
  } | null;
  elevation_range_centimeters: {
    minimum_centimeters: number;
    maximum_centimeters: number;
    level_zero_samples: number;
  };
  eviction_target: EvictionTarget | null;
};

export type CaptureInputs = {
  version: 2;
  prepared_revision: string;
  capture_revision: string;
  case_corrections: unknown;
  activation_cases: VisualCase[];
  cases: VisualCase[];
  eviction_case: VisualCase;
};

export type ActivationOutcome = {
  case_id: string;
  content_hash: string;
  start_available: boolean;
  result: string;
  message: string | null;
  capture: string | null;
  no_capture_reason: string | null;
  preparation_elapsed_milliseconds: number;
  page_count: number;
  page_bytes: number;
  package_chunk_count_bound: number;
  loaded_chunk_count: number;
  loaded_chunks: string[];
  elapsed_milliseconds: number;
};

const casesPath = process.env["AOE_SOURCE_VISUAL_CASES"];
export const evidenceDirectory = new URL(
  "../../reports/geographic-visuals/",
  import.meta.url,
);
const activeMap = (hash: string) => `Saved map active: ${hash.slice(0, 12)}`;

export function caseInputsPath() {
  if (!casesPath) throw new Error("missing AOE_SOURCE_VISUAL_CASES path");
  return casesPath;
}

export async function ready(page: Page) {
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-assets",
    "aoe2-local",
  );
}

async function loadedChunkCount(page: Page) {
  const text = (await page.locator("#terrain-cache").textContent()) ?? "";
  return Number(/\((\d+) \/ 512 chunks\)/.exec(text)?.[1] ?? 0);
}

export async function assetIdentity(page: Page) {
  const manifest = await page.evaluate(async () => {
    const response = await fetch("/asset-pack/manifest.json");
    return response.ok ? await response.json() : null;
  });
  if (manifest === null)
    return { manifest_status: "unavailable", manifest_sha256: null };
  const digest = createHash("sha256")
    .update(JSON.stringify(manifest))
    .digest("hex");
  return { manifest_status: "available", manifest_sha256: digest };
}

function blueLikePixels(image: Buffer) {
  const png = PNG.sync.read(image);
  let count = 0;
  for (let index = 0; index < png.data.length; index += 4) {
    const red = png.data[index] ?? 0;
    const green = png.data[index + 1] ?? 0;
    const blue = png.data[index + 2] ?? 0;
    if (blue > 70 && blue > red * 1.2 && blue > green * 1.05) count += 1;
  }
  return { width: png.width, height: png.height, blue_like_pixels: count };
}

function sha256Hex(bytes: number[]) {
  return bytes.map((value) => value.toString(16).padStart(2, "0")).join("");
}

export async function activateMatrixCase(
  page: Page,
  item: VisualCase,
  assetSource: string,
  asset: Awaited<ReturnType<typeof assetIdentity>>,
  inputs: CaptureInputs,
): Promise<ActivationOutcome> {
  const started = Date.now();
  const loadedChunks = new Set<string>();
  page.on("response", (response) => {
    const url = new URL(response.url());
    if (url.pathname.startsWith(`/maps/${item.content_hash}/chunks/`)) {
      loadedChunks.add(url.pathname);
    }
  });
  await page.goto("/");
  await ready(page);
  await page.getByRole("button", { name: "Open map creator" }).click();
  await page.getByLabel("Saved maps").selectOption(item.content_hash);
  await page.getByRole("button", { name: "Open saved" }).click();
  await expect
    .poll(() => page.locator("#map-estimate").textContent(), {
      timeout: 60_000,
    })
    .toMatch(/Saved map active:|Saved map available for preview:/);
  const message = (await page.locator("#map-estimate").textContent()) ?? "";
  const startAvailable = message.startsWith(activeMap(item.content_hash));
  const outputDirectory = new URL(`${item.id}/`, evidenceDirectory);
  await mkdir(outputDirectory, { recursive: true });
  let capture: string | null = null;
  let noCaptureReason: string | null = null;
  if (startAvailable) {
    await expect
      .poll(() => loadedChunkCount(page), { timeout: 30_000 })
      .toBeGreaterThan(0);
    const imagePath = new URL("webgpu-overview.png", outputDirectory);
    const metadataPath = new URL("webgpu-overview.json", outputDirectory);
    const image = await page.locator("#scene").screenshot();
    await writeFile(fileURLToPath(imagePath), image);
    await writeFile(
      fileURLToPath(metadataPath),
      JSON.stringify(
        {
          version: 1,
          case_id: item.id,
          location: item.location,
          content_hash: item.content_hash,
          manifest_path: item.manifest_path,
          request: item.request,
          schema_version: item.schema_version,
          generator_version: item.generator_version,
          generation_recipe_version: item.generation_recipe_version,
          source_locks: item.source_locks.map((lock) => ({
            ...lock,
            sha256: sha256Hex(lock.sha256),
          })),
          page_evidence: {
            water: item.water_pages,
            elevation_range_centimeters: item.elevation_range_centimeters,
            historical_land_use: item.historical_land_use,
          },
          preparation_elapsed_milliseconds:
            item.preparation_elapsed_milliseconds,
          page_count: item.page_count,
          page_bytes: item.page_bytes,
          package_chunk_count_bound: item.package_chunk_count_bound,
          renderer: "webgpu",
          asset_source: assetSource,
          asset_manifest: asset,
          capture_revision: inputs.capture_revision,
          prepared_revision: inputs.prepared_revision,
          camera: await page.locator("#world-position").textContent(),
          loaded_chunks: [...loadedChunks].sort(),
          activation: { start_available: true, message: null },
          screenshot: fileURLToPath(imagePath),
        },
        null,
        2,
      ),
    );
    capture = fileURLToPath(imagePath);
  } else {
    noCaptureReason = message.replace(
      /^Saved map available for preview:\s*/,
      "",
    );
  }
  return {
    case_id: item.id,
    content_hash: item.content_hash,
    start_available: startAvailable,
    result: startAvailable ? "activated" : "uninhabitable_preview_only",
    message: startAvailable ? null : noCaptureReason,
    capture,
    no_capture_reason: noCaptureReason,
    preparation_elapsed_milliseconds: item.preparation_elapsed_milliseconds,
    page_count: item.page_count,
    page_bytes: item.page_bytes,
    package_chunk_count_bound: item.package_chunk_count_bound,
    loaded_chunk_count: loadedChunks.size,
    loaded_chunks: [...loadedChunks].sort(),
    elapsed_milliseconds: Date.now() - started,
  };
}

function changedPixelCount(firstImage: Buffer, secondImage: Buffer) {
  const first = PNG.sync.read(firstImage);
  const second = PNG.sync.read(secondImage);
  if (first.width !== second.width || first.height !== second.height) {
    throw new Error("camera captures have different dimensions");
  }
  let changed = 0;
  for (let index = 0; index < first.data.length; index += 4) {
    if (
      first.data[index] !== second.data[index] ||
      first.data[index + 1] !== second.data[index + 1] ||
      first.data[index + 2] !== second.data[index + 2]
    ) {
      changed += 1;
    }
  }
  return changed;
}

export async function captureCase(
  page: Page,
  item: VisualCase,
  backend: "webgpu" | "canvas2d",
) {
  const assetSource = await gameAssets(page);
  const loadedChunks = new Set<string>();
  const chunkResponses = new Map<string, number>();
  page.on("response", (response) => {
    const url = new URL(response.url());
    if (
      response.status() === 200 &&
      url.pathname.startsWith(`/maps/${item.content_hash}/chunks/`)
    ) {
      loadedChunks.add(url.pathname);
      chunkResponses.set(
        url.pathname,
        (chunkResponses.get(url.pathname) ?? 0) + 1,
      );
    }
  });
  await page.goto("/");
  const asset = await assetIdentity(page);
  await ready(page);
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    backend,
  );
  await page.getByRole("button", { name: "Open map creator" }).click();
  await page.getByLabel("Saved maps").selectOption(item.content_hash);
  await page.getByRole("button", { name: "Open saved" }).click();
  await expect(page.locator("#map-estimate")).toContainText(
    activeMap(item.content_hash),
    {
      timeout: 30_000,
    },
  );
  await expect
    .poll(() => loadedChunkCount(page), { timeout: 30_000 })
    .toBeGreaterThan(0);
  const canvas = page.locator("#scene");
  const initialCamera = await page.locator("#world-position").textContent();
  if (item.eviction_target) {
    const inputs = JSON.parse(
      await readFile(caseInputsPath(), "utf8"),
    ) as CaptureInputs;
    await captureEvictionCase(
      page,
      item,
      backend,
      chunkResponses,
      assetSource,
      asset,
      inputs,
      initialCamera,
      new URL(`${item.id}/`, evidenceDirectory),
    );
    return;
  }

  const initialImage = await canvas.screenshot();
  await page.getByRole("button", { name: "Center on primary unit" }).click();
  const box = await canvas.boundingBox();
  if (!box) throw new Error(`${item.id} ${backend} canvas is missing`);
  await canvas.click({ position: { x: box.width / 2, y: box.height / 2 } });
  await expect(page.locator("#unit-state")).toHaveText(
    /^unit (idle|moving|planning)$/,
    {
      timeout: 10_000,
    },
  );
  await canvas.click({
    position: { x: box.width / 2 + 90, y: box.height / 2 + 24 },
    button: "right",
  });
  const beforePanCamera = await page.locator("#world-position").textContent();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down({ button: "middle" });
  await page.mouse.move(
    box.x + box.width / 2 + 75,
    box.y + box.height / 2 + 35,
  );
  await page.mouse.up();
  const afterPanCamera = await page.locator("#world-position").textContent();
  expect(afterPanCamera).not.toBe(beforePanCamera);
  const pannedImage = await canvas.screenshot();
  await page.mouse.wheel(0, -240);
  const zoomedImage = await canvas.screenshot();
  const zoomedPixelChanges = changedPixelCount(pannedImage, zoomedImage);
  expect(zoomedPixelChanges).toBeGreaterThan(100);
  const traversedCamera = afterPanCamera;
  await expect(page.locator("#world-position")).not.toHaveText(
    initialCamera ?? "",
  );
  await expect(page.locator("#connection")).toHaveText("connected");
  await page.getByRole("button", { name: "Reconnect to server" }).click();
  await ready(page);
  await page.reload();
  await ready(page);
  await page.getByRole("button", { name: "Open map creator" }).click();
  await page.getByLabel("Saved maps").selectOption(item.content_hash);
  await page.getByRole("button", { name: "Open saved" }).click();
  await expect(page.locator("#map-estimate")).toContainText(
    activeMap(item.content_hash),
    {
      timeout: 30_000,
    },
  );
  await expect
    .poll(() => loadedChunkCount(page), { timeout: 30_000 })
    .toBeGreaterThan(0);
  const finalCamera = await page.locator("#world-position").textContent();
  const image = await canvas.screenshot();
  const caseDirectory = new URL(`${item.id}/`, evidenceDirectory);
  await mkdir(caseDirectory, { recursive: true });
  const screenshot = new URL(`${backend}.png`, caseDirectory);
  const before = new URL(`${backend}-initial.png`, caseDirectory);
  const traversed = new URL(`${backend}-traversed.png`, caseDirectory);
  const zoomed = new URL(`${backend}-zoomed.png`, caseDirectory);
  const metadata = new URL(`${backend}.json`, caseDirectory);
  await writeFile(fileURLToPath(screenshot), image);
  await writeFile(fileURLToPath(before), initialImage);
  await writeFile(fileURLToPath(traversed), pannedImage);
  await writeFile(fileURLToPath(zoomed), zoomedImage);
  await writeFile(
    fileURLToPath(metadata),
    JSON.stringify(
      {
        version: 1,
        case_id: item.id,
        location: item.location,
        content_hash: item.content_hash,
        manifest_path: item.manifest_path,
        request: item.request,
        schema_version: item.schema_version,
        generator_version: item.generator_version,
        generation_recipe_version: item.generation_recipe_version,
        source_locks: item.source_locks.map((lock) => ({
          ...lock,
          sha256: sha256Hex(lock.sha256),
        })),
        page_evidence: {
          water: item.water_pages,
          elevation_range_centimeters: item.elevation_range_centimeters,
          historical_land_use: item.historical_land_use,
        },
        renderer: backend,
        asset_source: assetSource,
        asset_manifest: asset,
        capture_revision: JSON.parse(await readFile(caseInputsPath(), "utf8"))
          .capture_revision,
        prepared_revision: JSON.parse(await readFile(caseInputsPath(), "utf8"))
          .prepared_revision,
        camera: {
          initial: initialCamera,
          after_pan_zoom: traversedCamera,
          final: finalCamera,
        },
        loaded_chunks: [...loadedChunks].sort(),
        interactions: {
          selected_primary_unit: true,
          move_order_submitted: true,
          panned: beforePanCamera !== traversedCamera,
          zoomed: zoomedPixelChanges > 100,
          zoom_changed_pixels: zoomedPixelChanges,
          reconnected: true,
          reloaded: true,
          eviction: `not-exercised: package grid bound ${item.package_chunk_count_bound} is below the 512 chunk cache limit`,
          package_chunk_count_bound: item.package_chunk_count_bound,
        },
        screenshot: fileURLToPath(screenshot),
        initial_screenshot: fileURLToPath(before),
        traversed_screenshot: fileURLToPath(traversed),
        zoomed_screenshot: fileURLToPath(zoomed),
        pixels: {
          initial: blueLikePixels(initialImage),
          final: blueLikePixels(image),
        },
      },
      null,
      2,
    ),
  );
  expect(loadedChunks.size).toBeGreaterThan(0);
  expect(item.source_locks.length).toBeGreaterThan(0);
}
