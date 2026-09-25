import { expect, type Page } from "@playwright/test";
import { PNG } from "pngjs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { syntheticSurfaceColors } from "./game-assets.js";

type CaptureKind = "gameplay" | "surface";

export async function waitForValidSelection(page: Page) {
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect(page.locator("#unit-state")).toHaveText(
    /^unit (idle|moving|planning)$/,
  );
  await expect
    .poll(async () => {
      const text = (await page.locator("#terrain-cache").textContent()) ?? "";
      return Number(/\((\d+) \/ 512 chunks\)/.exec(text)?.[1] ?? 0);
    })
    .toBeGreaterThan(0);
}

export async function saveSelectedCapture(
  page: Page,
  image: Buffer,
  project: string,
  evidence: string,
  kind: CaptureKind,
): Promise<string> {
  await waitForValidSelection(page);
  const pixels = PNG.sync.read(image);
  const ring = colorPixels(pixels, syntheticSurfaceColors.ring, 12);
  expect(ring, "post-interaction selection ring pixels").toBeGreaterThan(24);

  const privateAssets = evidence === "local AoE II pack";
  const directory = new URL(
    privateAssets ? "../../local-assets/evidence/" : "../../reports/e2e/",
    import.meta.url,
  );
  const prefix = kind === "gameplay" ? "gameplay" : "aoeworld-map";
  const scope = privateAssets ? "private-" : "";
  const capture = new URL(`${prefix}-${scope}${project}.png`, directory);
  await mkdir(directory, { recursive: true });
  await writeFile(capture, image);
  expect((await readFile(capture)).equals(image)).toBe(true);
  return fileURLToPath(capture);
}

function colorPixels(
  image: PNG,
  expected: readonly number[],
  tolerance: number,
) {
  let count = 0;
  for (let offset = 0; offset < image.data.length; offset += 4) {
    if (
      expected.every(
        (value, index) =>
          Math.abs((image.data[offset + index] ?? 0) - value) <= tolerance,
      )
    ) {
      count += 1;
    }
  }
  return count;
}

export function syntheticTerrainCoverage(image: PNG) {
  const expected = [
    syntheticSurfaceColors.grass,
    syntheticSurfaceColors.grassAccent,
  ];
  let covered = 0;
  let samples = 0;
  for (let y = 8; y < image.height - 8; y += 8) {
    for (let x = 8; x < image.width - 8; x += 8) {
      const offset = (y * image.width + x) * 4;
      samples += 1;
      if (
        expected.some((color) =>
          color.every(
            (value, index) => (image.data[offset + index] ?? 0) === value,
          ),
        )
      ) {
        covered += 1;
      }
    }
  }
  return covered / samples;
}
