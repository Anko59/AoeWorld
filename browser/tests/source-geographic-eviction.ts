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
  await repeatCameraKey(page, "ArrowDown", 450);
  await page.waitForLoadState("networkidle");
  await expect
    .poll(() => responses.get(targetPath) ?? 0, { timeout: 45_000 })
    .toBeGreaterThan(0);
  const initialTargetImage = await canvas.screenshot();

  // Read the camera limits through the normal controls, then visit a regular
  // world-coordinate grid. Screen-horizontal keys move along an isometric
  // diagonal, so simply alternating arrows does not cover the square map.
  const southeast = await cameraCenter(page);
  await repeatCameraKey(page, "ArrowUp", 450);
  await page.waitForLoadState("networkidle");
  const northwest = await cameraCenter(page);
  // Finish the whole tour. Crossing 512 once only evicts a few chunks;
  // the southeast target need not be among them under spatial retention.
  for (let row = 0; row < 13; row++) {
    for (let column = 0; column < 13; column++) {
      const xIndex = row % 2 === 0 ? column : 12 - column;
      await panToWorld(page, [
        northwest[0] + ((southeast[0] - northwest[0]) * xIndex) / 12,
        northwest[1] + ((southeast[1] - northwest[1]) * row) / 12,
      ]);
      // Let the renderer request and install the visible chunks at each
      // stop; racing hundreds of keys only renders the final camera position.
      await page.waitForLoadState("networkidle", { timeout: 30_000 });
    }
  }
  await expect
    .poll(() => uniqueChunkCount(responses, contentHash), { timeout: 60_000 })
    .toBeGreaterThan(512);
  await expect
    .poll(() => residentChunkCount(page), { timeout: 30_000 })
    .toBe(512);
  const cacheResidentChunksAfterScan = await residentChunkCount(page);
  // The tour ends in the southeast. Reload the distant northwest views,
  // including the height-shifted interior band, so spatial eviction must
  // displace the southeast target rather than an unrelated corner.
  await repeatCameraKey(page, "ArrowUp", 450);
  for (const offset of [0, 32, 64, 96]) {
    await panToWorld(page, [northwest[0] + offset, northwest[1] + offset]);
    await page.waitForLoadState("networkidle");
  }
  const evictedImage = await canvas.screenshot();
  const targetRequestCountBeforeReturn = responses.get(targetPath) ?? 0;
  await repeatCameraKey(page, "ArrowDown", 450);
  await expect
    .poll(() => responses.get(targetPath) ?? 0, { timeout: 45_000 })
    .toBeGreaterThan(targetRequestCountBeforeReturn);
  await page.waitForLoadState("networkidle");
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
  key: "ArrowLeft" | "ArrowRight" | "ArrowDown" | "ArrowUp",
  count: number,
  settle = true,
) {
  if (count <= 0) return;
  await page.evaluate(
    async ({ key, count, settle }) => {
      for (let index = 0; index < count; index++) {
        document.dispatchEvent(
          new KeyboardEvent("keydown", { key, bubbles: true }),
        );
        if ((index + 1) % 32 === 0) {
          await new Promise<void>((resolve) => window.setTimeout(resolve, 0));
        }
      }
      if (settle) {
        await new Promise<void>((resolve) =>
          requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
        );
      }
    },
    { key, count, settle },
  );
}

async function cameraCenter(page: Page): Promise<[number, number]> {
  const text = await page.locator("#world-position").textContent();
  const coordinates = (text ?? "").split(",").map(Number);
  const [x, y] = coordinates;
  if (
    coordinates.length !== 2 ||
    x === undefined ||
    y === undefined ||
    !Number.isFinite(x) ||
    !Number.isFinite(y)
  ) {
    throw new Error("camera readout is not a coordinate pair");
  }
  return [x, y];
}

async function panToWorld(page: Page, target: [number, number]) {
  for (let attempt = 0; attempt < 80; attempt++) {
    const current = await cameraCenter(page);
    const dx = target[0] - current[0];
    const dy = target[1] - current[1];
    if (Math.max(Math.abs(dx), Math.abs(dy)) < 3) return;
    const scale = Math.min(1, 24 / Math.max(Math.abs(dx), Math.abs(dy)));
    // At minimum zoom (0.25), one vertical key moves both world axes by
    // 3.75 tiles; one horizontal key moves them oppositely by 1.875 tiles.
    const vertical = Math.round(((dx + dy) * scale) / 7.5);
    const horizontal = Math.round(((dx - dy) * scale) / 3.75);
    await repeatCameraKey(
      page,
      vertical >= 0 ? "ArrowDown" : "ArrowUp",
      Math.abs(vertical),
      false,
    );
    await repeatCameraKey(
      page,
      horizontal >= 0 ? "ArrowRight" : "ArrowLeft",
      Math.abs(horizontal),
      false,
    );
    await page.evaluate(
      () =>
        new Promise<void>((resolve) =>
          requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
        ),
    );
  }
  throw new Error(
    `camera did not reach source traversal stop ${target.join(",")}`,
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
