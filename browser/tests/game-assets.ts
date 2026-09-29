import type { Page } from "@playwright/test";
import { PNG } from "pngjs";

const terrainSourceColors = {
  grass: [70, 120, 55],
  ramp: [215, 72, 45],
  cliff: [92, 82, 105],
  water: [32, 104, 210],
  shore: [218, 185, 92],
} as const;

const terrainSourceAccents = {
  grass: [42, 82, 38],
  ramp: [170, 34, 22],
  dirt: [115, 54, 30],
  cliff: [44, 42, 64],
  water: [80, 160, 220],
  shore: [164, 119, 54],
} as const;

const tint = (source: readonly number[], factor: number) =>
  source.map((value) => Math.round(value * factor));
const blendWater = (source: readonly number[]) =>
  source.map((value, index) =>
    Math.round(value * 0.86 + ([38, 113, 190][index] ?? 0) * 0.14),
  );

export const syntheticSurfaceColors = {
  grass: terrainSourceColors.grass,
  grassAccent: terrainSourceAccents.grass,
  ramp: tint(terrainSourceColors.ramp, 0.92),
  rampAccent: tint(terrainSourceAccents.ramp, 0.92),
  cliff: tint(terrainSourceColors.cliff, 0.78),
  cliffAccent: tint(terrainSourceAccents.cliff, 0.78),
  skirt: tint(terrainSourceColors.cliff, 0.72),
  water: blendWater(terrainSourceColors.water),
  waterAccent: blendWater(terrainSourceAccents.water),
  shore: terrainSourceColors.shore,
  shoreAccent: terrainSourceAccents.shore,
  ring: [242, 217, 89],
} as const;

type TerrainSource = {
  x: number;
  y: number;
  width: number;
  height: number;
  base: readonly number[];
  accent: readonly number[];
  frameCount: number;
  tiled?: boolean;
};

const terrainSources = new Map<number, TerrainSource>([
  [
    15008,
    {
      x: 0,
      y: 0,
      width: 97,
      height: 49,
      base: terrainSourceColors.grass,
      accent: terrainSourceAccents.grass,
      frameCount: 100,
      tiled: true,
    },
  ],
  [
    15007,
    {
      x: 0,
      y: 160,
      width: 25,
      height: 45,
      base: terrainSourceColors.ramp,
      accent: terrainSourceAccents.ramp,
      frameCount: 10,
    },
  ],
  [
    15000,
    {
      x: 0,
      y: 600,
      width: 25,
      height: 45,
      base: [166, 92, 48],
      accent: terrainSourceAccents.dirt,
      frameCount: 100,
      tiled: true,
    },
  ],
  [
    15010,
    {
      x: 1_000,
      y: 60,
      width: 25,
      height: 45,
      base: terrainSourceColors.shore,
      accent: terrainSourceAccents.shore,
      frameCount: 10,
    },
  ],
  [
    15018,
    {
      x: 1_000,
      y: 120,
      width: 25,
      height: 45,
      base: terrainSourceColors.cliff,
      accent: terrainSourceAccents.cliff,
      frameCount: 10,
    },
  ],
  [
    15002,
    {
      x: 1_000,
      y: 180,
      width: 25,
      height: 45,
      base: terrainSourceColors.water,
      accent: terrainSourceAccents.water,
      frameCount: 10,
    },
  ],
]);

/** Public CI uses original generated fixtures; local runs use the real pack. */
export async function gameAssets(
  page: Page,
  forceSynthetic = false,
): Promise<string> {
  if (!forceSynthetic) {
    const response = await page.request.get("/asset-pack/manifest.json");
    if (response.ok()) return "local AoE II pack";
  }
  const color = new PNG({ width: 2048, height: 2048 });
  const shadow = new PNG({ width: 2048, height: 2048 });
  const pixel = (x: number, y: number, rgb: readonly number[]) => {
    const offset = (y * 2048 + x) * 4;
    rgb.forEach((value, i) => (color.data[offset + i] = value));
    color.data[offset + 3] = 255;
  };
  for (const source of terrainSources.values()) {
    for (let frame = 0; frame < source.frameCount; frame += 1) {
      for (let y = 0; y < source.height; y += 1) {
        for (let x = 0; x < source.width; x += 1) {
          const accent = (x * 2 + y + frame) % 9 === 0;
          const frameX = source.tiled ? frame % 10 : frame;
          const frameY = source.tiled ? Math.floor(frame / 10) : 0;
          pixel(
            source.x + frameX * source.width + x,
            source.y + frameY * source.height + y,
            accent ? source.accent : source.base,
          );
        }
      }
    }
  }
  for (let y = 80; y < 125; y += 1)
    for (let x = 1_300; x < 1_325; x += 1) pixel(x, y, [65, 145, 245]);
  for (let y = 80; y < 125; y += 1)
    for (let x = 1_340; x < 1_365; x += 1) pixel(x, y, [235, 40, 200]);
  for (let y = 116; y < 123; y += 1)
    for (let x = 1_500; x < 1_519; x += 1) {
      const dx = (x - 1_509) / 9;
      const dy = (y - 119.5) / 3.5;
      if (dx * dx + dy * dy <= 1) shadow.data[(y * 2048 + x) * 4 + 3] = 128;
    }
  const frames = [
    ["graphics", 3008, 50],
    ["graphics", 3004, 50],
    ...[15008, 15007, 15000, 15010, 15018, 15002].map((id) => [
      "terrain",
      id,
      id === 15008 || id === 15000 ? 100 : 10,
    ]),
    ["graphics", 435, 4],
    ["graphics", 4652, 14],
    ["graphics", 2296, 14],
  ].flatMap(([archive, id, count]) => {
    const source = terrainSources.get(Number(id));
    return Array.from({ length: Number(count) }, (_, frame) => ({
      source: `${archive}.drs:[32, 112, 108, 115]:${id}`,
      source_hash: "fixture",
      frame,
      page: 0,
      x: source
        ? source.x + (source.tiled ? frame % 10 : frame) * source.width
        : Number(id) === 2296
          ? 1_500
          : Number(id) === 4652
            ? 1_340
            : 1_300,
      y: source
        ? source.y + (source.tiled ? Math.floor(frame / 10) : 0) * source.height
        : 80,
      width: source?.width ?? 25,
      height: source?.height ?? 45,
      anchor_x: source ? 0 : 12,
      anchor_y: source ? 0 : 42,
    }));
  });
  const manifest = {
    version: 1,
    converter: "test fixture",
    input_hash: "fixture",
    frames,
    pages: [
      {
        color: "fixture-color.png",
        player: "fixture-mask.png",
        shadow: "fixture-shadow.png",
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
  const shadows = PNG.sync.write(shadow);
  const empty = PNG.sync.write(new PNG({ width: 2048, height: 2048 }));
  await page.route("**/asset-pack/*", async (route) => {
    const url = route.request().url();
    if (url.endsWith("manifest.json")) {
      await route.fulfill({ json: manifest });
    } else {
      await route.fulfill({
        contentType: "image/png",
        body: url.endsWith("fixture-color.png")
          ? colored
          : url.endsWith("fixture-shadow.png")
            ? shadows
            : empty,
      });
    }
  });
  return "generated CI fixtures";
}
