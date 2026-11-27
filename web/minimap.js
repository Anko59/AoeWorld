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
  ['grass', [118, 150, 76]],
  ['dry grass', [166, 152, 98]],
  ['forest', [34, 74, 44]],
  ['soil', [140, 112, 78]],
  ['sand', [206, 190, 142]],
  ['rock', [132, 126, 120]],
  ['snow', [236, 239, 236]],
  ['ice', [194, 218, 228]],
  ['shore', [98, 164, 178]],
  ['river', [75, 131, 171]],
  ['lake', [49, 109, 154]],
  ['ocean', [28, 73, 111]],
  ['cropland', [176, 156, 88]],
  ['pasture', [140, 154, 90]],
];
// A class under a relief shade; lit pale classes pass 255. Shade 8 is the exact
// class colour. The power curve lifts the one- and two-step slopes that make up
// nearly all real relief; water is always level.
export function minimapTone(kind, shade) {
  const slope = kind >= 8 && kind <= 11 ? 0 : (shade - 8) / 8;
  const light = Math.sign(slope) * Math.abs(slope) ** 0.62;
  const tint = light > 0 ? [255, 244, 206] : [22, 28, 50];
  const weight = (light > 0 ? 0.22 : 0.62) * Math.abs(light);
  const keep = (light > 0 ? 1 + 0.42 * light : 1) * (1 - weight);
  return MINIMAP_CLASSES[kind][1].map((value, at) => value * keep + tint[at] * weight);
}
// RGBA of every sample byte; the two unused classes stay wholly transparent.
const tones = new Uint8ClampedArray(256 * 4);
MINIMAP_CLASSES.forEach((_, kind) => {
  for (let shade = 0; shade < 16; shade++) {
    tones.set([...minimapTone(kind, shade), 255], (kind | shade << 4) * 4);
  }
});
const toneWords = new Uint32Array(tones.buffer);

// Returns {samples, rgba, cells} for a valid raster and null for anything else; a
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
  const cells = bytes.subarray(MINIMAP_HEADER_BYTES);
  const rgba = new Uint8ClampedArray(samples * samples * 4), words = new Uint32Array(rgba.buffer);
  // Each sample decides alone: no neighbour's class can show through the fog.
  for (let index = 0; index < cells.length; index++) {
    const tone = toneWords[cells[index]];
    if (tone === 0) return null;
    words[index] = tone;
  }
  return { samples, rgba, cells };
}

// Normalized isometric map coordinates, before the SVG square's letterboxing.
export function minimapProject(point, world, view = { zoom: 1, center: world.map(v => v / 2) }) {
  const dx = (point[0] - view.center[0]) / world[0];
  const dy = (point[1] - view.center[1]) / world[1];
  return [128 + 128 * view.zoom * (dx - dy), 128 + 128 * view.zoom * (dx + dy)];
}

// A stretched map fills its whole rectangle (the native menu's 2:1 rhombus);
// otherwise the square map is letterboxed inside it.
function unclampedPoint(rect, clientX, clientY, world,
  view = { zoom: 1, center: world.map(v => v / 2) }, stretch = false) {
  const side = Math.min(rect.width, rect.height);
  if (!(side > 0)) return null;
  const [w, h] = stretch ? [rect.width, rect.height] : [side, side];
  const x = ((clientX - rect.left - (rect.width - w) / 2) / w - 0.5) / view.zoom;
  const y = ((clientY - rect.top - (rect.height - h) / 2) / h - 0.5) / view.zoom;
  return [view.center[0] + (x + y) * world[0], view.center[1] + (y - x) * world[1]];
}

export function minimapWorldPoint(rect, clientX, clientY, world, view, stretch) {
  const point = unclampedPoint(rect, clientX, clientY, world, view, stretch);
  return point && point.map((v, i) => Math.max(0, Math.min(world[i] - 1, v)));
}

// Null outside the world: a clamped order would march to its border.
export function minimapOrderPoint(rect, clientX, clientY, world, view, stretch) {
  const point = unclampedPoint(rect, clientX, clientY, world, view, stretch);
  return point?.every((v, i) => v >= 0 && v < world[i]) ? point : null;
}

// Five-number actor records: kind and flags, world x/y, owner, packed RGB.
// The low nibble flags buildings (1), economic units (2), ships (4), and local
// ownership of a unit or a building (8).
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
        units: 0, workers: 0, buildings: 0, world: [0, 0], owners: new Map() };
      pixels.set(key, pixel);
    }
    pixel.count++;
    pixel[kind & 1 ? 'buildings' : 'units']++;
    if (kind & 2) pixel.workers++;
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

const colours = new Map();
export function actorColour(colour) {
  let text = colours.get(colour);
  if (text === undefined) {
    text = `rgb(${[(colour >> 16) & 255, (colour >> 8) & 255, colour & 255].join(',')})`;
    if (colours.size < 256) colours.set(colour, text);
  }
  return text;
}
