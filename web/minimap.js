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

// Normalized isometric map coordinates, before the SVG square's letterboxing.
export function minimapProject(point, world, view = { zoom: 1, center: world.map(v => v / 2) }) {
  const dx = (point[0] - view.center[0]) / world[0];
  const dy = (point[1] - view.center[1]) / world[1];
  return [128 + 128 * view.zoom * (dx - dy), 128 + 128 * view.zoom * (dx + dy)];
}

export function minimapWorldPoint(rect, clientX, clientY, world,
  view = { zoom: 1, center: world.map(v => v / 2) }) {
  const side = Math.min(rect.width, rect.height);
  if (!(side > 0)) return null;
  const x = ((clientX - rect.left - (rect.width - side) / 2) / side - 0.5) / view.zoom;
  const y = ((clientY - rect.top - (rect.height - side) / 2) / side - 0.5) / view.zoom;
  return [Math.max(0, Math.min(world[0] - 1, view.center[0] + (x + y) * world[0])),
    Math.max(0, Math.min(world[1] - 1, view.center[1] + (y - x) * world[1]))];
}

// Five-number actor records: kind (unit/building), world x/y, owner, packed RGB.
// Work is proportional to the received actors and occupied screen pixels, never
// world area. Ties choose the smaller owner id, independent of arrival order.
export function aggregatePixels(actors, project, width, height, cell = 1) {
  const pixels = new Map();
  for (let at = 0; at + 4 < actors.length; at += 5) {
    const kind = actors[at], wx = actors[at + 1], wy = actors[at + 2];
    const owner = actors[at + 3], colour = actors[at + 4];
    if (![kind, wx, wy, owner, colour].every(Number.isFinite)) continue;
    const [sx, sy] = project([wx, wy]);
    if (sx < 0 || sy < 0 || sx >= width || sy >= height) continue;
    const x = Math.floor(sx / cell), y = Math.floor(sy / cell);
    const key = y * Math.ceil(width / cell) + x;
    let pixel = pixels.get(key);
    if (!pixel) {
      pixel = { key, x: (x + .5) * cell, y: (y + .5) * cell, count: 0,
        units: 0, buildings: 0, world: [0, 0], owners: new Map() };
      pixels.set(key, pixel);
    }
    pixel.count++;
    pixel[kind === 1 ? 'buildings' : 'units']++;
    pixel.world[0] += wx; pixel.world[1] += wy;
    const vote = pixel.owners.get(owner) ?? { count: 0, colour };
    vote.count++;
    pixel.owners.set(owner, vote);
  }
  return [...pixels.values()].sort((a, b) => a.key - b.key).map(pixel => {
    const [owner, vote] = [...pixel.owners].sort((a, b) => b[1].count - a[1].count || a[0] - b[0])[0];
    delete pixel.owners;
    return { ...pixel, owner, colour: vote.colour, world: pixel.world.map(v => v / pixel.count) };
  });
}

export function aggregateActors(actors, world, view, side) {
  side = Math.max(1, Math.min(2048, Math.floor(side)));
  const project = point => minimapProject(point, world, view).map(v => v * side / 256);
  return aggregatePixels(actors, project, side, side).map(pixel => ({ ...pixel,
    size: Math.max(.65, Math.min(5, (pixel.buildings ? 4 : 2) * side * view.zoom / Math.max(...world))) }));
}

export function actorColour(colour) {
  return `rgb(${[(colour >> 16) & 255, (colour >> 8) & 255, colour & 255].join(',')})`;
}
