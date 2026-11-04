// One cartographic surface per authoritative AWMM raster. No generated terrain,
// height, waterways or land-use guesses. Relief is the source-derived shade.
const palette = [
  [143,158,102], [185,172,111], [47,87,57], [155,124,84],
  [215,195,143], [146,140,126], [235,236,220], [180,207,209],
  [108,165,167], [54,116,142], [44,101,132], [32,74,105],
  [190,164,93], [151,166,99],
];
export function paintTerrain({ samples, cells }) {
  if (!Number.isInteger(samples) || samples < 1 || samples > 512 || cells?.length !== samples * samples) return null;
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = samples * 2;
  const context = canvas.getContext('2d');
  if (!context) return null;
  const side = canvas.width, pixels = new Uint8ClampedArray(side * side * 4);
  for (let y = 0; y < side; y++) for (let x = 0; x < side; x++) {
    const sx = x >> 1, sy = y >> 1, cell = cells[sy * samples + sx], kind = cell & 15;
    const base = palette[kind];
    if (!base) return null;
    const shade = 1 + ((cell >> 4) - 8) * .075;
    // Canopy stippling and water glints are ink, confined to measured classes.
    const ink = kind === 2 ? ((x + y) % 3 === 0 ? .88 : 1.04)
      : kind === 12 ? (y % 2 ? .88 : 1.08) : 1;
    const at = (y * side + x) * 4;
    for (let c = 0; c < 3; c++) pixels[at + c] = base[c] * shade * ink;
    pixels[at + 3] = 255;
    // A fine bank only where the raster actually joins land and water.
    if (kind >= 8 && kind <= 11 && ((sx > 0 && ((cells[sy * samples + sx - 1] & 15) < 8 || (cells[sy * samples + sx - 1] & 15) > 11) && x % 2 === 0)
      || (sy > 0 && (cells[(sy - 1) * samples + sx] & 15) < 8 && y % 2 === 0))) {
      pixels[at] *= .78; pixels[at + 1] *= .85; pixels[at + 2] *= .9;
    }
  }
  context.putImageData(new ImageData(pixels, side, side), 0, 0);
  return canvas;
}
