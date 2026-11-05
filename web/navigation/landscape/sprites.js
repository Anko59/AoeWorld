// Real building art for the far map. The pack's own atlas pages hold the
// sprites; this module cuts the handful of semantic frames it needs (one
// composite per building kind), tints the player-colour pixels, and keeps
// only those small canvases. The 5 MB frame manifest is parsed once, off the
// critical path, and dropped; nothing here runs per frame.
const parts = {
  0: [889, 891], 1: [2223], 2: [3507], 3: [3495], 4: [3483], 6: [2683], 8: [4330], 9: [376],
  10: [24], 11: [1009], 12: [93], 13: [2278], 14: [2655], 16: [305], 17: [281], 18: [3835, 1363], 19: [958],
};
const BAKE = 160;
const baked = new Map(), tinted = new Map();
let state = 'idle', wake = () => {};

export const spriteReady = () => state === 'ready';
export function onSprites(callback) { wake = callback; }

async function load(path) {
  const image = new Image(); image.src = path; await image.decode(); return image;
}
async function bake() {
  state = 'loading';
  try {
    const manifest = await fetch('/asset-pack/manifest.json').then(r => r.ok ? r.json() : null);
    if (!manifest?.frames) { state = 'missing'; return; }
    const wanted = new Set(Object.values(parts).flat().map(String)), frames = new Map();
    for (const f of manifest.frames) {
      const slp = String(f.source).split(':').pop();
      if (wanted.has(slp) && f.native === null && !frames.has(slp)) frames.set(slp, f);
    }
    const pages = new Map();
    await Promise.all([...new Set([...frames.values()].map(f => f.page))].map(async page => {
      const n = String(page).padStart(3, '0');
      pages.set(page, await Promise.all([load(`/asset-pack/color-${n}.png`), load(`/asset-pack/player-${n}.png`)]));
    }));
    for (const [kind, slps] of Object.entries(parts)) {
      const list = slps.map(s => frames.get(String(s))).filter(Boolean);
      if (!list.length) continue;
      const left = Math.min(...list.map(f => -f.anchor_x)), top = Math.min(...list.map(f => -f.anchor_y));
      const right = Math.max(...list.map(f => f.width - f.anchor_x)), bottom = Math.max(...list.map(f => f.height - f.anchor_y));
      const w = right - left, h = bottom - top, k = Math.min(1, BAKE / Math.max(w, h));
      // Two layers at the baked size: the colour art and a player-colour mask.
      const make = () => { const c = document.createElement('canvas'); c.width = Math.ceil(w * k); c.height = Math.ceil(h * k); return c; };
      const art = make(), mask = make();
      for (const f of list) {
        const [color, player] = pages.get(f.page);
        const dx = (-f.anchor_x - left) * k, dy = (-f.anchor_y - top) * k;
        art.getContext('2d').drawImage(color, f.x, f.y, f.width, f.height, dx, dy, f.width * k, f.height * k);
        mask.getContext('2d').drawImage(player, f.x, f.y, f.width, f.height, dx, dy, f.width * k, f.height * k);
      }
      baked.set(Number(kind), { art, mask, ox: -left * k, oy: -top * k, native: w });
    }
    state = 'ready'; wake();
  } catch { state = 'missing'; }
}
export function loadSprites() { if (state === 'idle') { state = 'queued'; (window.requestIdleCallback ?? setTimeout)(bake); } }

// The player mask stores a red shade step wherever the sprite shows its
// owner; the tint multiplies the baked grey by the owner colour.
export function buildingSprite(kind, colour) {
  const base = baked.get(kind);
  if (!base) return null;
  const key = kind * 16777216 + (colour & 0xffffff);
  let result = tinted.get(key);
  if (result) return result;
  const { art, mask } = base, canvas = document.createElement('canvas');
  canvas.width = art.width; canvas.height = art.height;
  const context = canvas.getContext('2d', { willReadFrequently: true });
  context.drawImage(art, 0, 0);
  const pixels = context.getImageData(0, 0, canvas.width, canvas.height);
  const m = mask.getContext('2d', { willReadFrequently: true }).getImageData(0, 0, canvas.width, canvas.height).data;
  const tr = (colour >> 16) & 255, tg = (colour >> 8) & 255, tb = colour & 255, d = pixels.data;
  for (let i = 0; i < d.length; i += 4) {
    if (m[i + 3] < 128 || m[i] === 0) continue;
    const shade = Math.min(1.5, ((d[i] + d[i + 1] + d[i + 2]) / 3) / 110);
    d[i] = Math.min(255, tr * shade); d[i + 1] = Math.min(255, tg * shade); d[i + 2] = Math.min(255, tb * shade);
  }
  context.putImageData(pixels, 0, 0);
  result = { canvas, width: canvas.width, height: canvas.height, ox: base.ox, oy: base.oy, native: base.native };
  if (tinted.size < 256) tinted.set(key, result);
  return result;
}
