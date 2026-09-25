import type { Page } from "@playwright/test";

const CHUNK_TILES = 32;

export async function activateSyntheticMap(page: Page): Promise<string> {
  const contentHash = "01".repeat(32);
  const chunk = JSON.stringify([
    syntheticChunk(255, 255),
    syntheticChunk(256, 255),
    syntheticChunk(255, 256),
    syntheticChunk(256, 256),
  ]);
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
  return contentHash;
}

function syntheticChunk(
  x: number,
  y: number,
): {
  x: number;
  y: number;
  payload_hex: string;
} {
  const bytes = [1, ...u16(CHUNK_TILES * CHUNK_TILES), ...u16(0)];
  for (let localY = 0; localY < CHUNK_TILES; localY += 1) {
    for (let localX = 0; localX < CHUNK_TILES; localX += 1) {
      const tileX = x * CHUNK_TILES + localX;
      const tileY = y * CHUNK_TILES + localY;
      const corners = tileCorners(tileX, tileY);
      const level = Math.round(
        corners.reduce((sum, value) => sum + value, 0) / corners.length,
      );
      bytes.push(...i32(level * 100), ...i16(level));
      for (const height of corners) bytes.push(...i16(height));
      bytes.push(...u32(properties(tileX, tileY)));
    }
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
  const material = ramp ? 1 : cliff ? 6 : water ? 11 : transition ? 10 : 0;
  const waterKind = water ? 1 : transition ? 2 : 0;
  const surfaceKind = ramp ? 1 : cliff ? 2 : 0;
  const diagonal = (x + y) & 1;
  return (
    material |
    (2 << 4) |
    (3 << 8) |
    (waterKind << 11) |
    (3 << 14) |
    (3 << 17) |
    ((waterKind === 0 && surfaceKind !== 2 ? 1 : 0) << 20) |
    (surfaceKind << 21) |
    (diagonal << 23)
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
