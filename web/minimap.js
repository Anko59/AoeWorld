// Strategic raster contract of GET /maps/{hash}/minimap: "AWMM", a
// little-endian axis (1..512), two zero bytes, then one byte per sample in
// row-major order with the terrain class in the low nibble and the relief
// shade (0 dark .. 8 level .. 15 lit) in the high nibble.
export const MAX_MINIMAP_SAMPLES = 512;
export const MINIMAP_HEADER_BYTES = 8;
export const MAX_MINIMAP_BYTES = MINIMAP_HEADER_BYTES + MAX_MINIMAP_SAMPLES ** 2;
// Forest is deliberately much darker than every open-ground colour, and the
// three water kinds share one hue family that no land class uses.
export const MINIMAP_CLASSES = [
  ['grass', [124, 164, 82]],
  ['dry grass', [176, 166, 96]],
  ['forest', [34, 84, 44]],
  ['soil', [142, 112, 76]],
  ['sand', [216, 196, 136]],
  ['rock', [134, 128, 122]],
  ['snow', [238, 242, 246]],
  ['ice', [196, 224, 236]],
  ['shore', [108, 168, 184]],
  ['river', [66, 138, 196]],
  ['lake', [52, 112, 170]],
  ['ocean', [30, 70, 118]],
];

// Returns {samples, rgba} for a valid raster and null for anything else; a
// rejected raster leaves the schematic map in place.
export function decodeMinimap(buffer) {
  if (!(buffer instanceof ArrayBuffer) || buffer.byteLength < MINIMAP_HEADER_BYTES
    || buffer.byteLength > MAX_MINIMAP_BYTES) return null;
  const bytes = new Uint8Array(buffer);
  if (bytes[0] !== 65 || bytes[1] !== 87 || bytes[2] !== 77 || bytes[3] !== 77
    || bytes[6] !== 0 || bytes[7] !== 0) return null;
  const samples = bytes[4] | bytes[5] << 8;
  if (samples < 1 || samples > MAX_MINIMAP_SAMPLES
    || bytes.length !== MINIMAP_HEADER_BYTES + samples * samples) return null;
  const rgba = new Uint8ClampedArray(samples * samples * 4);
  for (let index = 0; index < samples * samples; index++) {
    const cell = bytes[MINIMAP_HEADER_BYTES + index];
    const color = MINIMAP_CLASSES[cell & 15]?.[1];
    if (!color) return null;
    // 8 is level ground; each step is 4.5% brighter or darker.
    const light = 1 + ((cell >> 4) - 8) * 0.045;
    rgba[index * 4] = color[0] * light;
    rgba[index * 4 + 1] = color[1] * light;
    rgba[index * 4 + 2] = color[2] * light;
    rgba[index * 4 + 3] = 255;
  }
  return { samples, rgba };
}

// Pointer position inside the letterboxed square map → clamped world tile
// position. The raster always spans the whole world on both axes.
export function minimapWorldPoint(rect, clientX, clientY, world) {
  const side = Math.min(rect.width, rect.height);
  if (!(side > 0)) return null;
  const left = rect.left + (rect.width - side) / 2;
  const top = rect.top + (rect.height - side) / 2;
  // Invert the same translate/rotate/scale used by #minimap-frame.
  const x = (clientX - left) / side - 0.5;
  const y = (clientY - top) / side - 0.5;
  return [Math.max(0, Math.min(world[0] - 1, (x + y + 0.5) * world[0])),
    Math.max(0, Math.min(world[1] - 1, (y - x + 0.5) * world[1]))];
}
