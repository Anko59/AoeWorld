// The land a player knows: the plain raster seen through the 256-cell fog mask.
// A pixel reads its own sample and the fog around its own cell, and a pixel of
// an unexplored cell stays empty, so no scaling can bring hidden land back.
import { MAX_MINIMAP_SAMPLES, MINIMAP_CLASSES, minimapTone } from '/minimap.js';

const SIDE = 512, AXIS = 256, CELLS = AXIS * AXIS;
const VEIL = [66, 74, 88], RIM = [255, 246, 214];
// Both looks of every raster byte: land in sight is lifted, remembered land is
// dim under a cool veil.
const remembered = new Float32Array(256 * 3), visible = new Float32Array(256 * 3);
const lumaOf = ([red, green, blue]) => red * 0.213 + green * 0.715 + blue * 0.072;
// Lifted snow would be wholly white: what passes the knee is squeezed below
// white, so that only its steepest lit slope reaches it.
const KNEE = 234;
function squeeze(lifted, peak) {
  return lifted > KNEE ? (KNEE + (lifted - KNEE) * (255 - KNEE) / (peak - KNEE)) / lifted : 1;
}
MINIMAP_CLASSES.forEach(([name], kind) => {
  const snow = name === 'snow', peak = lumaOf(minimapTone(kind, 15)) * 1.14;
  for (let shade = 0; shade < 16; shade++) {
    const tone = minimapTone(kind, shade).map(value => snow ? value : Math.min(255, value));
    const luma = lumaOf(tone), lift = 1.14 * (snow ? squeeze(luma * 1.14, peak) : 1);
    tone.forEach((value, channel) => {
      const at = (kind | shade << 4) * 3 + channel, vivid = luma + (value - luma) * 1.1;
      remembered[at] = vivid * 0.64 + VEIL[channel] * 0.16;
      visible[at] = Math.min(255, vivid * lift);
    });
  }
});
// A map without land is one sample of an unused class: flat, warm and grey.
const NO_LAND = Uint8Array.of(15);
remembered.set([78, 68, 58], NO_LAND[0] * 3);
visible.set([112, 98, 84], NO_LAND[0] * 3);
function opaque(colours) {
  const bytes = new Uint8ClampedArray(256 * 4);
  for (let at = 0; at < 256; at++) bytes.set([...colours.subarray(at * 3, at * 3 + 3), 255], at * 4);
  return new Uint32Array(bytes.buffer);
}
const rememberedWords = opaque(remembered), visibleWords = opaque(visible);
// A pixel is a quarter of a cell: bilinear weights of 16 towards its own corner.
const blend = (plane, cell, dx, dy) =>
  plane[cell] * 9 + (plane[cell + dx] + plane[cell + dy]) * 3 + plane[cell + dx + dy];

export function initKnownLand(terrain, mask) {
  const layer = document.createElementNS('http://www.w3.org/2000/svg', 'foreignObject');
  layer.id = 'minimap-known';
  for (const name of ['width', 'height']) layer.setAttribute(name, String(AXIS));
  layer.setAttribute('pointer-events', 'none');
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = SIDE;
  canvas.style.display = 'block';
  canvas.style.width = canvas.style.height = `${AXIS}px`;
  layer.append(canvas);
  terrain.after(layer);
  const context = canvas.getContext('2d');
  const image = context?.createImageData(SIDE, SIDE);
  const words = image && new Uint32Array(image.data.buffer);
  const [state, inner, field, sight, rim, flat, row] = Array.from({ length: 7 }, () => new Int8Array(CELLS));
  const column = new Uint16Array(SIDE);
  let cells = NO_LAND, samples = 1, fresh = true, disabled = false, waiting = false;
  layer.dataset.workingBytes = String((image?.data.byteLength ?? 0) + CELLS * 7
    + column.byteLength + MAX_MINIMAP_SAMPLES ** 2);

  // The bits that a cell shares with all its neighbours inside the world.
  function shared(from, to) {
    for (let cell = 0; cell < CELLS; cell++) {
      const x = cell % AXIS;
      row[cell] = from[cell] & (x > 0 ? from[cell - 1] : -1) & (x < AXIS - 1 ? from[cell + 1] : -1);
    }
    for (let cell = 0; cell < CELLS; cell++) {
      to[cell] = row[cell] & (cell >= AXIS ? row[cell - AXIS] : -1)
        & (cell < CELLS - AXIS ? row[cell + AXIS] : -1);
    }
  }

  function readMask() {
    for (let cell = 0; cell < CELLS; cell++) {
      const alpha = mask[cell * 4 + 3];
      state[cell] = alpha === 255 ? 0 : alpha === 0 ? 3 : 1; // 1 explored, 2 in sight
    }
    shared(state, inner);
    for (let cell = 0; cell < CELLS; cell++) flat[cell] = ~inner[cell] & 1;
    shared(flat, flat);
    for (let cell = 0; cell < CELLS; cell++) {
      sight[cell] = state[cell] >> 1;
      rim[cell] = (state[cell] & ~inner[cell]) >> 1;
      // Opacity in quarters: a cell beside the unexplored is half faded, unless
      // it has no unfaded neighbour (thin trails survive).
      field[cell] = state[cell] === 0 ? -1 : inner[cell] & 1 || flat[cell] ? 4 : 2;
    }
    for (let cell = 0; cell < CELLS; cell++)
      flat[cell] = field[cell] < 4 || rim[cell] ? 0 : sight[cell] ? 2 : 1;
    // What agrees with its whole neighbourhood needs no blending.
    shared(flat, flat);
  }

  function compose() {
    waiting = false;
    if (fresh || disabled) return;
    readMask();
    const bytes = image.data;
    for (let y = 0, pixel = 0; y < SIDE; y++) {
      const from = column[y] * samples, first = (y >> 1) * AXIS;
      const dy = y & 1 ? (y < SIDE - 1 ? AXIS : 0) : (y > 0 ? -AXIS : 0);
      for (let x = 0; x < SIDE; x++, pixel++) {
        const cell = first + (x >> 1);
        if (state[cell] === 0) { words[pixel] = 0; continue; }
        const sample = cells[from + column[x]];
        if (flat[cell]) {
          words[pixel] = flat[cell] === 2 ? visibleWords[sample] : rememberedWords[sample];
          continue;
        }
        const dx = x & 1 ? (x < SIDE - 1 ? 1 : 0) : (x > 0 ? -1 : 0);
        const seen = blend(sight, cell, dx, dy) / 16;
        const glow = blend(rim, cell, dx, dy) / 16 * seen * 0.4;
        for (let channel = 0; channel < 3; channel++) {
          const dim = remembered[sample * 3 + channel];
          const colour = dim + (visible[sample * 3 + channel] - dim) * seen;
          bytes[pixel * 4 + channel] = colour + (RIM[channel] - colour) * glow;
        }
        bytes[pixel * 4 + 3] = blend(field, cell, dx, dy) / 64 * 255;
      }
    }
    context.putImageData(image, 0, 0);
  }

  function refresh() {
    if (waiting || fresh || disabled || !words || !mask) return;
    waiting = true;
    requestAnimationFrame(compose);
  }

  return {
    // The mask changed, or the fog was switched.
    fogged(off) {
      fresh = false;
      disabled = off;
      layer.style.display = off ? 'none' : '';
      refresh();
    },
    // The map's decoded raster. Null is a new map: the canvas is emptied at
    // once and stays so until its first fog, which then shows the flat tone.
    land(raster) {
      cells = raster?.cells ?? NO_LAND;
      samples = raster?.samples ?? 1;
      for (let at = 0; at < SIDE; at++) column[at] = Math.floor((at + 0.5) * samples / SIDE);
      if (raster) { refresh(); return; }
      fresh = true;
      context?.clearRect(0, 0, SIDE, SIDE);
    },
  };
}
