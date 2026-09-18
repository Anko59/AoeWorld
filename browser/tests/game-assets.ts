import type { Page } from "@playwright/test";
import { PNG } from "pngjs";

/** Public CI uses original generated fixtures; local runs use the real pack. */
export async function gameAssets(page: Page): Promise<string> {
  const response = await page.request.get("/asset-pack/manifest.json");
  if (response.ok()) return "local AoE II pack";
  const color = new PNG({ width: 2048, height: 2048 });
  const pixel = (x: number, y: number, rgb: number[]) => {
    const offset = (y * 2048 + x) * 4;
    rgb.forEach((value, i) => (color.data[offset + i] = value));
    color.data[offset + 3] = 255;
  };
  for (let y = 0; y < 49; y += 1) {
    for (let x = 0; x < 97; x += 1) {
      if (Math.abs(x - 48) / 48 + Math.abs(y - 24) / 24 <= 1.03)
        pixel(x, y, [70, 120, 55]);
    }
  }
  for (let y = 80; y < 125; y += 1)
    for (let x = 10; x < 35; x += 1) pixel(x, y, [65, 145, 245]);
  const frames = [
    ["graphics", 3008, 50],
    ["graphics", 3004, 50],
    ["terrain", 15008, 10],
    ["graphics", 435, 4],
  ].flatMap(([archive, id, count]) =>
    Array.from({ length: Number(count) }, (_, frame) => ({
      source: `${archive}.drs:[32, 112, 108, 115]:${id}`,
      source_hash: "fixture",
      frame,
      page: 0,
      x: id === 15008 ? 0 : 10,
      y: id === 15008 ? 0 : 80,
      width: id === 15008 ? 97 : 25,
      height: id === 15008 ? 49 : 45,
      anchor_x: id === 15008 ? 0 : 12,
      anchor_y: id === 15008 ? 0 : 42,
    })),
  );
  const manifest = {
    version: 1,
    converter: "test fixture",
    input_hash: "fixture",
    frames,
    pages: [
      {
        color: "fixture-color.png",
        player: "fixture-mask.png",
        shadow: "fixture-mask.png",
        outline: "fixture-mask.png",
        color_hash: "fixture",
        player_hash: "fixture",
        shadow_hash: "fixture",
        outline_hash: "fixture",
        width: 2048,
        height: 2048,
      },
    ],
  };
  const colored = PNG.sync.write(color);
  const empty = PNG.sync.write(new PNG({ width: 2048, height: 2048 }));
  await page.route("**/asset-pack/*", async (route) => {
    const url = route.request().url();
    if (url.endsWith("manifest.json")) {
      await route.fulfill({ json: manifest });
    } else {
      await route.fulfill({
        contentType: "image/png",
        body: url.endsWith("fixture-color.png") ? colored : empty,
      });
    }
  });
  return "generated CI fixtures";
}
