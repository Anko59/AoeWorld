import { expect, type Page } from "@playwright/test";
import { PNG } from "pngjs";

export type EvictionTarget = {
  chunk_x: number;
  chunk_y: number;
  minimum_elevation_centimeters: number;
  maximum_elevation_centimeters: number;
};

export type EvictionTraversal = {
  initial_target_image: Buffer;
  evicted_image: Buffer;
  returned_image: Buffer;
  unique_chunk_request_count: number;
  target_chunk_path: string;
  target_request_count_before_return: number;
  target_request_count_after_return: number;
  cache_resident_chunks_after_scan: number;
  cache_resident_chunks_after_return: number;
  visual_similarity_percent: number;
  visual_restored: boolean;
  loaded_chunks: string[];
};

export async function traverseAndReturnAlpineTarget(
  page: Page,
  contentHash: string,
  target: EvictionTarget,
  responses: Map<string, number>,
): Promise<EvictionTraversal> {
  const canvas = page.locator("#scene");
  const bounds = await canvas.boundingBox();
  if (!bounds) throw new Error("Alpine eviction canvas is missing");
  await page.mouse.move(
    bounds.x + bounds.width / 2,
    bounds.y + bounds.height / 2,
  );
  await page.mouse.wheel(0, 5_000);
  const targetPath = `/maps/${contentHash}/chunks/${target.chunk_x}/${target.chunk_y}`;

  // The active map opens at its center. At minimum zoom, this moves the same
  // camera to the southeast relief chunk without reloading or changing maps.
  await repeatCameraKey(page, "ArrowLeft", 220);
  await expect
    .poll(() => responses.get(targetPath) ?? 0, { timeout: 45_000 })
    .toBeGreaterThan(0);
  const initialTargetImage = await canvas.screenshot();

  // Sweep parallel diagonal rows across the package. The camera stays on this
  // package, so exceeding capacity forces normal client LRU eviction.
  await repeatCameraKey(page, "ArrowRight", 450);
  let direction: "ArrowUp" | "ArrowDown" = "ArrowUp";
  for (
    let row = 0;
    row < 24 && uniqueChunkCount(responses, contentHash) <= 512;
    row++
  ) {
    await repeatCameraKey(page, direction, 400);
    await page.waitForTimeout(150);
    if (uniqueChunkCount(responses, contentHash) > 512) break;
    if (direction === "ArrowUp") {
      await repeatCameraKey(page, "ArrowLeft", 16);
      direction = "ArrowDown";
    } else {
      await repeatCameraKey(page, "ArrowRight", 16);
      direction = "ArrowUp";
    }
  }
  await expect
    .poll(() => uniqueChunkCount(responses, contentHash), { timeout: 60_000 })
    .toBeGreaterThan(512);
  await expect
    .poll(() => residentChunkCount(page), { timeout: 30_000 })
    .toBe(512);
  const cacheResidentChunksAfterScan = await residentChunkCount(page);
  const evictedImage = await canvas.screenshot();

  // Settle at the northwest edge before recording the pre-return request
  // count, then traverse back to the southeast target on the same session.
  await repeatCameraKey(page, "ArrowRight", 450);
  await page.waitForTimeout(750);
  const targetRequestCountBeforeReturn = responses.get(targetPath) ?? 0;
  await repeatCameraKey(page, "ArrowLeft", 450);
  await expect
    .poll(() => responses.get(targetPath) ?? 0, { timeout: 45_000 })
    .toBeGreaterThan(targetRequestCountBeforeReturn);
  await page.waitForTimeout(300);
  const returnedImage = await canvas.screenshot();
  const cacheResidentChunksAfterReturn = await residentChunkCount(page);
  const visualSimilarityPercent = screenshotSimilarityPercent(
    initialTargetImage,
    returnedImage,
  );
  const visualRestored = visualSimilarityPercent >= 60;
  expect(visualRestored).toBe(true);

  return {
    initial_target_image: initialTargetImage,
    evicted_image: evictedImage,
    returned_image: returnedImage,
    unique_chunk_request_count: uniqueChunkCount(responses, contentHash),
    target_chunk_path: targetPath,
    target_request_count_before_return: targetRequestCountBeforeReturn,
    target_request_count_after_return: responses.get(targetPath) ?? 0,
    cache_resident_chunks_after_scan: cacheResidentChunksAfterScan,
    cache_resident_chunks_after_return: cacheResidentChunksAfterReturn,
    visual_similarity_percent: visualSimilarityPercent,
    visual_restored: visualRestored,
    loaded_chunks: [...responses.keys()]
      .filter((path) => path.startsWith(`/maps/${contentHash}/chunks/`))
      .sort(),
  };
}

async function repeatCameraKey(
  page: Page,
  key: "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown",
  count: number,
) {
  await page.evaluate(
    async ({ key, count }) => {
      for (let index = 0; index < count; index++) {
        document.dispatchEvent(
          new KeyboardEvent("keydown", { key, bubbles: true }),
        );
        if ((index + 1) % 32 === 0) {
          await new Promise<void>((resolve) => window.setTimeout(resolve, 0));
        }
      }
    },
    { key, count },
  );
}

function uniqueChunkCount(responses: Map<string, number>, hash: string) {
  const prefix = `/maps/${hash}/chunks/`;
  return [...responses.keys()].filter((path) => path.startsWith(prefix)).length;
}

function residentChunkCount(page: Page) {
  return page
    .locator("#terrain-cache")
    .textContent()
    .then((text) =>
      Number(/\((\d+) \/ 512 chunks\)/.exec(text ?? "")?.[1] ?? 0),
    );
}

function screenshotSimilarityPercent(firstImage: Buffer, secondImage: Buffer) {
  const first = PNG.sync.read(firstImage);
  const second = PNG.sync.read(secondImage);
  if (first.width !== second.width || first.height !== second.height) return 0;
  let compared = 0;
  let similar = 0;
  for (let index = 0; index < first.data.length; index += 16) {
    compared += 1;
    if (
      Math.abs((first.data[index] ?? 0) - (second.data[index] ?? 0)) <= 16 &&
      Math.abs((first.data[index + 1] ?? 0) - (second.data[index + 1] ?? 0)) <=
        16 &&
      Math.abs((first.data[index + 2] ?? 0) - (second.data[index + 2] ?? 0)) <=
        16
    ) {
      similar += 1;
    }
  }
  return compared === 0 ? 0 : Math.round((similar * 100) / compared);
}
