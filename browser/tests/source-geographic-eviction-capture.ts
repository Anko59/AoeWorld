import { mkdir, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { expect, type Page } from "@playwright/test";
import { traverseAndReturnAlpineTarget } from "./source-geographic-eviction.js";
import type {
  CaptureInputs,
  VisualCase,
} from "./source-geographic-visual-capture.js";

type AssetIdentity = {
  manifest_status: string;
  manifest_sha256: string | null;
};

export async function captureEvictionCase(
  page: Page,
  item: VisualCase,
  backend: "webgpu" | "canvas2d",
  responses: Map<string, number>,
  assetSource: string,
  asset: AssetIdentity,
  inputs: CaptureInputs,
  initialCamera: string | null,
  caseDirectory: URL,
) {
  const target = item.eviction_target;
  if (!target)
    throw new Error("Alpine eviction case has no source relief target");
  const traversal = await traverseAndReturnAlpineTarget(
    page,
    item.content_hash,
    target,
    responses,
  );
  const returnedCamera = await page.locator("#world-position").textContent();
  await mkdir(caseDirectory, { recursive: true });
  const screenshot = new URL(`${backend}.png`, caseDirectory);
  const initial = new URL(`${backend}-initial.png`, caseDirectory);
  const evicted = new URL(`${backend}-evicted.png`, caseDirectory);
  const returned = new URL(`${backend}-returned.png`, caseDirectory);
  const metadata = new URL(`${backend}.json`, caseDirectory);
  await writeFile(fileURLToPath(screenshot), traversal.returned_image);
  await writeFile(fileURLToPath(initial), traversal.initial_target_image);
  await writeFile(fileURLToPath(evicted), traversal.evicted_image);
  await writeFile(fileURLToPath(returned), traversal.returned_image);
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
          southeast_chunk_elevation_centimeters: {
            minimum: target.minimum_elevation_centimeters,
            maximum: target.maximum_elevation_centimeters,
          },
          historical_land_use: item.historical_land_use,
        },
        renderer: backend,
        asset_source: assetSource,
        asset_manifest: asset,
        capture_revision: inputs.capture_revision,
        prepared_revision: inputs.prepared_revision,
        camera: { initial: initialCamera, returned: returnedCamera },
        loaded_chunks: traversal.loaded_chunks,
        unique_chunk_request_count: traversal.unique_chunk_request_count,
        target_chunk_path: traversal.target_chunk_path,
        target_request_count_before_return:
          traversal.target_request_count_before_return,
        target_request_count_after_return:
          traversal.target_request_count_after_return,
        cache_resident_chunks_after_scan:
          traversal.cache_resident_chunks_after_scan,
        cache_resident_chunks_after_return:
          traversal.cache_resident_chunks_after_return,
        eviction_target: target,
        interactions: {
          eviction_exercised: true,
          visual_restored: traversal.visual_restored,
          visual_similarity_percent: traversal.visual_similarity_percent,
          returned_to_target: true,
          cache_limit_chunks: 512,
        },
        screenshot: fileURLToPath(screenshot),
        initial_screenshot: fileURLToPath(initial),
        evicted_screenshot: fileURLToPath(evicted),
        returned_screenshot: fileURLToPath(returned),
      },
      null,
      2,
    ),
  );
  expect(traversal.unique_chunk_request_count).toBeGreaterThan(512);
  expect(traversal.target_request_count_after_return).toBeGreaterThan(
    traversal.target_request_count_before_return,
  );
  expect(traversal.cache_resident_chunks_after_scan).toBeLessThanOrEqual(512);
  expect(traversal.cache_resident_chunks_after_return).toBeLessThanOrEqual(512);
  expect(item.source_locks.length).toBeGreaterThan(0);
}

function sha256Hex(bytes: number[]) {
  return bytes.map((value) => value.toString(16).padStart(2, "0")).join("");
}
