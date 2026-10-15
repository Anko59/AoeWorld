import type { Page } from "@playwright/test";

const CHUNK_TILES = 32;

/** `true` plants a tree on every tile of flat ground; a predicate picks tiles. */
export type SyntheticTrees = boolean | ((x: number, y: number) => boolean);

export async function activateSyntheticMap(
  page: Page,
  denseResources: SyntheticTrees = false,
  center: readonly [number, number] = [8192, 8192],
): Promise<string> {
  const chunkX = Math.round(center[0] / CHUNK_TILES);
  const chunkY = Math.round(center[1] / CHUNK_TILES);
  const contentHash = "01".repeat(32);
  const chunk = JSON.stringify(
    denseResources === true
      ? Array.from({ length: 81 }, (_, index) =>
          syntheticChunk(
            chunkX - 4 + (index % 9),
            chunkY - 4 + Math.floor(index / 9),
            true,
          ),
        )
      : [
          [-1, -1],
          [0, -1],
          [-1, 0],
          [0, 0],
        ].map(([x = 0, y = 0]) =>
          syntheticChunk(chunkX + x, chunkY + y, denseResources),
        ),
  );
  await page.evaluate(
    async ([hash, chunkJson]) => {
      const module = (await import(
        new URL("/pkg/aoe_client.js", location.href).href
      )) as {
        activate_surface_fixture(hash: string, chunk: string): void;
      };
      module.activate_surface_fixture(hash, chunkJson);
    },
    [contentHash, chunk] as const,
  );
  if (center[0] === 8192 && center[1] === 8192)
    await page.keyboard.press("Home");
  return contentHash;
}

export function syntheticChunk(
  x: number,
  y: number,
  denseResources: SyntheticTrees = false,
): {
  x: number;
  y: number;
  payload_hex: string;
} {
  // Chunk format 5: header, then per tile 20 terrain bytes, explicit world
  // coordinates and 7 appearance bytes; resources carry a visual family.
  const trees: [number, number][] = [];
  const planted = denseResources === true ? () => true : denseResources;
  for (let tileY = y * CHUNK_TILES; tileY < (y + 1) * CHUNK_TILES; tileY++)
    for (let tileX = x * CHUNK_TILES; tileX < (x + 1) * CHUNK_TILES; tileX++)
      if (planted && planted(tileX, tileY)) trees.push([tileX, tileY]);
  const bytes = [
    5,
    ...u16(CHUNK_TILES * CHUNK_TILES),
    ...u16(trees.length),
    ...u16(0),
  ];
  for (let localY = 0; localY < CHUNK_TILES; localY += 1) {
    for (let localX = 0; localX < CHUNK_TILES; localX += 1) {
      const tileX = x * CHUNK_TILES + localX;
      const tileY = y * CHUNK_TILES + localY;
      const corners = denseResources ? [0, 0, 0, 0] : tileCorners(tileX, tileY);
      const level = Math.round(
        corners.reduce((sum, value) => sum + value, 0) / corners.length,
      );
      bytes.push(...i32(level * 100), ...i16(level));
      for (const height of corners) bytes.push(...i16(height));
      bytes.push(...u32(denseResources ? 1 << 21 : properties(tileX, tileY)));
      // No hydrology/land-cover evidence; open temperate lowland appearance.
      bytes.push(...u16(0), ...i32(tileX), ...i32(tileY));
      bytes.push(...u16(0), ...u16(0), 0, 1, 0);
    }
  }
  for (const [tx, ty] of trees) {
    const id = BigInt(ty) * 524288n + BigInt(tx) * 2n;
    for (let b = 0n; b < 8n; b++) bytes.push(Number((id >> (8n * b)) & 255n));
    // Wood tree, 100 units, variant 0, broadleaf family.
    bytes.push(...i32(tx), ...i32(ty), 1, 0, ...u16(100), 0, 1);
  }
  return { x, y, payload_hex: bytes.map(hexByte).join("") };
}

function tileCorners(x: number, y: number): number[] {
  const surface = surfaceClass(x, y);
  if (surface === "cliff") return [2, 2, 2, 2];
  if (surface === "ramp") return [0, 1, 1, 0];
  return [0, 0, 0, 0];
}

function properties(x: number, y: number): number {
  const surface = surfaceClass(x, y);
  const cliff = surface === "cliff";
  const ramp = surface === "ramp";
  const water = surface === "water";
  const transition = surface === "shore";
  // Five material bits: lush = 2, grass/dirt = 14, worn = 15, gravel = 16.
  const material = ramp ? 1 : cliff ? 6 : water ? 11 : transition ? 10 : 0;
  const waterKind = water ? 1 : transition ? 2 : 0;
  const surfaceKind = ramp ? 1 : cliff ? 2 : 0;
  const diagonal = (x + y) & 1;
  return (
    material |
    (2 << 5) |
    (3 << 9) |
    (waterKind << 12) |
    (3 << 15) |
    (3 << 18) |
    ((waterKind === 0 && surfaceKind !== 2 ? 1 : 0) << 21) |
    (surfaceKind << 22) |
    (diagonal << 24)
  );
}

function surfaceClass(x: number, y: number) {
  const diagonal = x - 8_192 - (y - 8_192);
  const band = x + y - 16_384;
  if (diagonal >= -2 && diagonal <= 2 && band >= -9 && band <= -5)
    return "cliff";
  if (diagonal >= -9 && diagonal <= -5 && band >= -2 && band <= 2)
    return "ramp";
  if (diagonal >= 5 && diagonal <= 9 && band >= -2 && band <= 2) return "water";
  if (diagonal >= 5 && diagonal <= 9 && band >= 3 && band <= 7) return "shore";
  return "grass";
}

function u16(value: number): number[] {
  return [value & 0xff, (value >>> 8) & 0xff];
}

function i16(value: number): number[] {
  return u16(value & 0xffff);
}

function u32(value: number): number[] {
  return [
    value & 0xff,
    (value >>> 8) & 0xff,
    (value >>> 16) & 0xff,
    (value >>> 24) & 0xff,
  ];
}

function i32(value: number): number[] {
  return u32(value | 0);
}

function hexByte(value: number): string {
  return value.toString(16).padStart(2, "0");
}
