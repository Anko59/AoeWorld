import { expect, type Page } from "@playwright/test";
import { test } from "../../../server.js";
import { apart, contrast, luminance } from "./land-page.js";

/** MINIMAP_CLASSES, in order; 8 to 11 are water. */
const palette: [string, [number, number, number]][] = [
  ["grass", [118, 150, 76]],
  ["dry grass", [166, 152, 98]],
  ["forest", [34, 74, 44]],
  ["soil", [140, 112, 78]],
  ["sand", [206, 190, 142]],
  ["rock", [132, 126, 120]],
  ["snow", [236, 239, 236]],
  ["ice", [194, 218, 228]],
  ["shore", [98, 164, 178]],
  ["river", [75, 131, 171]],
  ["lake", [49, 109, 154]],
  ["ocean", [28, 73, 111]],
  ["cropland", [176, 156, 88]],
  ["pasture", [140, 154, 90]],
];
const water = (kind: number) => kind >= 8 && kind <= 11;

/** Every sample's RGBA through the served decoder; null when refused. */
async function decode(
  page: Page,
  rasters: { samples: number; cells: number[] }[],
) {
  await page.goto("/diagnostics.html");
  return page.evaluate(async (rasters) => {
    const { decodeMinimap } = (await import("/minimap.js" as string)) as {
      decodeMinimap(buffer: ArrayBuffer): { rgba: Uint8ClampedArray } | null;
    };
    return rasters.map(({ samples, cells }) => {
      const bytes = new Uint8Array(8 + cells.length);
      bytes.set([65, 87, 77, 77, samples & 255, samples >> 8]);
      bytes.set(cells, 8);
      const raster = decodeMinimap(bytes.buffer);
      if (!raster) return null;
      const colours: number[][] = [];
      for (let at = 0; at < raster.rgba.length; at += 4)
        colours.push(Array.from(raster.rgba.slice(at, at + 4)));
      return colours;
    });
  }, rasters);
}

/** [class][shade], read from one raster that holds every pair once. */
async function tones(page: Page) {
  const cells = Array.from({ length: 256 }, (_, at) =>
    at < 224 ? (at >> 4) | ((at & 15) << 4) : 0x80,
  );
  const [colours] = await decode(page, [{ samples: 16, cells }]);
  if (!colours) throw new Error("A valid class and shade is refused");
  return palette.map((_, kind) =>
    Array.from({ length: 16 }, (_, shade) => colours[kind * 16 + shade] ?? []),
  );
}

test("level land decodes to the exact table and every sample decides alone", async ({
  page,
}) => {
  let seed = 7;
  const random = (below: number) => {
    seed = (Math.imul(seed, 1103515245) + 12345) & 0x7fffffff;
    return (seed >>> 8) % below;
  };
  const noise = Array.from(
    { length: 64 * 64 },
    () => random(14) | (random(16) << 4),
  );
  const drawn = await decode(page, [
    ...palette.map((_, kind) => ({
      samples: 7,
      cells: Array<number>(49).fill(kind | (8 << 4)),
    })),
    { samples: 64, cells: noise },
  ]);
  for (const [kind, [name, colour]] of palette.entries()) {
    const found = new Set(drawn[kind]?.map((value) => value.join()));
    expect([...found], name).toEqual([[...colour, 255].join()]);
  }
  // No neighbour is read, so no coast can show through the fog.
  const byte = await tones(page);
  const scattered = drawn[palette.length] ?? [];
  expect(scattered).toHaveLength(noise.length);
  const strays = noise.filter(
    (cell, at) =>
      byte[cell & 15]?.[cell >> 4]?.join() !== scattered[at]?.join(),
  );
  expect(strays).toEqual([]);

  const level = (name: string) =>
    drawn[palette.findIndex(([label]) => label === name)]?.[0] ?? [];
  const hues = ["river", "lake", "ocean"].map((name) => {
    const [red = 0, green = 0, blue = 0] = level(name);
    return ((red - green) / (blue - Math.min(red, green)) + 4) * 60;
  });
  expect(Math.max(...hues) - Math.min(...hues), "one blue").toBeLessThan(5);
  for (const name of ["grass", "dry grass", "soil", "sand", "cropland"]) {
    expect(contrast(level("forest"), level(name)), name).toBeGreaterThan(2);
  }
});

test("relief lights land in order, water stays level, bad rasters are refused", async ({
  page,
}) => {
  const byte = await tones(page);
  for (const [kind, [name, colour]] of palette.entries()) {
    const shades = byte[kind] ?? [];
    expect(shades[8], name).toEqual([...colour, 255]);
    if (water(kind)) {
      expect(new Set(shades.map((shade) => shade.join())).size, name).toBe(1);
      continue;
    }
    const light = shades.map((shade) => luminance(shade));
    expect(light, name).toEqual([...light].sort((a, b) => a - b));
    expect(light[0] ?? NaN, name).toBeLessThan(light[8] ?? NaN);
    expect(light[8] ?? NaN, name).toBeLessThan(light[15] ?? NaN);
  }
  // u = (s - 8) / 8, t = sign(u)|u|^.62: one step from level already shows.
  for (const [kind, shade, colour] of [
    [0, 0, [58, 74, 60]],
    [0, 7, [102, 129, 72]],
    [0, 9, [139, 172, 92]],
    [0, 15, [182, 215, 126]],
    [2, 0, [27, 45, 48]],
    [2, 15, [89, 131, 90]],
  ] as const) {
    const found = byte[kind]?.[shade] ?? [];
    expect(apart(found, colour), `${kind}:${shade}`).toBeLessThanOrEqual(1);
  }

  const flat = Array<number>(64).fill(0x80);
  const broken = (at: number, cell: number) => ({
    samples: 8,
    cells: flat.map((value, index) => (index === at ? cell : value)),
  });
  const refused = await decode(page, [
    ...[14, 15].flatMap((kind) =>
      [0, 31, 63].map((at) => broken(at, kind | (8 << 4))),
    ),
    broken(63, 14 | (15 << 4)),
    { samples: 8, cells: flat.slice(1) },
    { samples: 7, cells: flat },
    { samples: 0, cells: [] },
    { samples: 513, cells: flat },
  ]);
  expect(refused).toEqual(Array<null>(11).fill(null));
});
