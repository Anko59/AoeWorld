import type { Page } from "@playwright/test";
import { PNG } from "pngjs";

export async function bluePixels(canvas: ReturnType<Page["locator"]>) {
  const image = PNG.sync.read(await canvas.screenshot());
  let blue = 0;
  for (let i = 0; i < image.data.length; i += 4) {
    const y = Math.floor(i / 4 / image.width);
    // Interface icons also use blue; only count pixels in the visible world.
    if (y < 54 || y >= image.height - 208) continue;
    const r = image.data[i] ?? 0;
    const g = image.data[i + 1] ?? 0;
    const b = image.data[i + 2] ?? 0;
    if (b > 150 && b > r + 30 && b > g + 30) blue += 1;
  }
  return blue;
}

export function assetTerrainColorVariety(image: PNG) {
  const colors = new Set<string>();
  for (let y = 32; y < image.height - 96; y += 16) {
    for (let x = 32; x < image.width - 32; x += 16) {
      if (x > image.width - 260 && y < 80) continue;
      if (x < 620 && y > image.height - 110) continue;
      if (
        x > image.width / 2 - 50 &&
        x < image.width / 2 + 50 &&
        y > image.height / 2 - 80 &&
        y < image.height / 2 + 80
      ) {
        continue;
      }
      const offset = (y * image.width + x) * 4;
      colors.add(
        `${image.data[offset]},${image.data[offset + 1]},${image.data[offset + 2]}`,
      );
    }
  }
  return colors.size;
}
