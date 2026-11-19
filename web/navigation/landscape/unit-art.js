import { spritePixels } from '/navigation/landscape/sprites.js';

const SIDE = 64;

// One compact sprite per kind. Owner colour is carried by the token rim, so
// distinct player palettes do not multiply the number of bitmaps or decodes.
export function initUnitArt(ready) {
  let frames = [], sprites = new Map(), queue = Promise.resolve();
  const assets = { wasm: null, chunks: new Map(), pages: new Map() };
  async function bake([width, height, x, y, ...source]) {
    const frame = { width, height, x, y };
    if (source.length === 3) frame.bytes = source;
    else frame.images = source;
    assets.wasm ??= await import('/pkg/aoe_client.js');
    const pixels = await spritePixels(assets, frame, 0xe2c58b);

    // Idle cells can contain wide transparent padding and soft ground shadows.
    // Fit the figure to the miniature using the visible silhouette.
    let left = width, top = height, right = 0, bottom = 0;
    for (let py = 0, at = 3; py < height; py++) for (let px = 0; px < width; px++, at += 4) {
      if (pixels.data[at] < 128) continue;
      left = Math.min(left, px); top = Math.min(top, py);
      right = Math.max(right, px + 1); bottom = Math.max(bottom, py + 1);
    }
    if (right <= left || bottom <= top) return null;
    const input = document.createElement('canvas');
    input.width = width; input.height = height;
    input.getContext('2d').putImageData(pixels, 0, 0);
    const scale = Math.min(1, SIDE / Math.max(right - left, bottom - top));
    const canvas = document.createElement('canvas');
    canvas.width = Math.ceil((right - left) * scale);
    canvas.height = Math.ceil((bottom - top) * scale);
    const context = canvas.getContext('2d');
    context.imageSmoothingQuality = 'high';
    context.drawImage(input, left, top, right - left, bottom - top, 0, 0, canvas.width, canvas.height);
    return canvas;
  }
  return {
    update(rows) {
      frames = rows;
      sprites = new Map();
      ready();
    },
    get(kind) {
      const row = frames[kind];
      if (!row) return null;
      if (!sprites.has(kind)) {
        const cache = sprites;
        cache.set(kind, null);
        queue = queue.then(async () => {
          // Yield between crops, including when the native chunk is cached.
          await new Promise(resolve => setTimeout(resolve, 0));
          if (cache !== sprites) return;
          const sprite = await bake(row).catch(() => null);
          if (cache === sprites) {
            cache.set(kind, sprite);
            ready();
          }
        });
      }
      return sprites.get(kind);
    },
  };
}
