// Native building ids come from each observed building's age, including fog
// memories. Bake requested DAT compositions once; never decode on a draw frame.
const BAKE = 160, LIMIT = 256;
const recipes = new Map(), baked = new Map(), pending = new Set();
let state = 'idle', wake = () => {}, assets = null, queue = Promise.resolve();
export const spriteReady = () => state === 'ready';
export function onSprites(callback) { wake = callback; }

async function load() {
  state = 'loading';
  try {
    let prefix = '/asset-pack/buildings/';
    let response = await fetch(`${prefix}manifest.json`);
    if (response.status === 404) { prefix = '/asset-pack/'; response = await fetch(`${prefix}manifest.json`); }
    if (!response.ok) { state = 'missing'; return; }
    const bytes = new Uint8Array(await response.arrayBuffer());
    const wasm = await import('/pkg/aoe_client.js');
    const parts = wasm.strategic_building_parts(bytes);
    const manifest = JSON.parse(new TextDecoder().decode(bytes));
    for (let at = 0; at < parts.length;) {
      const id = parts[at++], count = parts[at++], list = [];
      for (let i = 0; i < count; i++) {
        const frame = manifest.frames[parts[at++]];
        list.push({ frame, dx: parts[at++], dy: parts[at++] });
      }
      if (list.length) recipes.set(id, list);
    }
    assets = { prefix, manifest: { chunks: manifest.chunks, pages: manifest.pages }, wasm, chunks: new Map(), pages: new Map() };
    state = 'ready'; wake();
  } catch { state = 'missing'; }
}
export function loadSprites() {
  if (state === 'idle') { state = 'queued'; (window.requestIdleCallback ?? setTimeout)(load); }
}
async function image(path) {
  const value = new Image(); value.src = path; await value.decode(); return value;
}
async function pixels(frame, colour) {
  const { prefix, manifest, wasm, chunks, pages } = assets;
  if (frame.native) {
    const n = frame.native;
    let chunk = chunks.get(n.chunk);
    if (!chunk) {
      const file = manifest.chunks[n.chunk]?.file;
      if (!file || !/^[a-zA-Z0-9_-]+\.bin$/.test(file)) throw Error('Invalid native thumbnail chunk');
      const response = await fetch(`${prefix}${file}`);
      if (!response.ok) throw Error('Missing native thumbnail chunk');
      chunk = new Uint8Array(await response.arrayBuffer());
      // Two encoded chunks only; decoded pixels are released after composition.
      if (chunks.size === 2) chunks.delete(chunks.keys().next().value);
      chunks.set(n.chunk, chunk);
    }
    const rgba = wasm.strategic_frame(chunk.subarray(n.offset, n.offset + n.length), frame.width, frame.height, colour);
    return new ImageData(new Uint8ClampedArray(rgba), frame.width, frame.height);
  }
  let pair = pages.get(frame.page);
  if (!pair) {
    const page = manifest.pages[frame.page];
    const valid = file => typeof file === 'string' && /^[a-zA-Z0-9_.-]+$/.test(file);
    if (!valid(page.color) || !valid(page.player)) throw Error('Invalid thumbnail page');
    pair = await Promise.all([image(`${prefix}${page.color}`), image(`${prefix}${page.player}`)]);
    if (pages.size === 2) pages.delete(pages.keys().next().value);
    pages.set(frame.page, pair);
  }
  const cut = source => {
    const c = document.createElement('canvas'); c.width = frame.width; c.height = frame.height;
    const ctx = c.getContext('2d', { willReadFrequently: true });
    ctx.drawImage(source, frame.x, frame.y, frame.width, frame.height, 0, 0, frame.width, frame.height);
    return ctx.getImageData(0, 0, frame.width, frame.height);
  };
  const output = cut(pair[0]), mask = cut(pair[1]).data, d = output.data;
  const team = [(colour >> 16) & 255, (colour >> 8) & 255, colour & 255];
  for (let i = 0; i < d.length; i += 4) {
    const weight = mask[i + 3], shade = .65 + Math.min(mask[i], 7) / 7 * .35;
    for (let ch = 0; ch < 3; ch++) d[i + ch] = Math.round((team[ch] * shade * weight + d[i + ch] * (255 - weight)) / 255);
    d[i + 3] = Math.round((255 * weight + d[i + 3] * (255 - weight)) / 255);
  }
  return output;
}
async function bake(id, colour, key) {
  const parts = recipes.get(id);
  const left = Math.min(...parts.map(p => p.dx - p.frame.anchor_x));
  const top = Math.min(...parts.map(p => p.dy - p.frame.anchor_y));
  const right = Math.max(...parts.map(p => p.dx + p.frame.width - p.frame.anchor_x));
  const bottom = Math.max(...parts.map(p => p.dy + p.frame.height - p.frame.anchor_y));
  const width = right - left, height = bottom - top, scale = Math.min(1, BAKE / Math.max(width, height));
  const canvas = document.createElement('canvas'); canvas.width = Math.ceil(width * scale); canvas.height = Math.ceil(height * scale);
  const context = canvas.getContext('2d');
  for (const p of parts) {
    const f = p.frame, source = document.createElement('canvas'); source.width = f.width; source.height = f.height;
    source.getContext('2d').putImageData(await pixels(f, colour), 0, 0);
    context.drawImage(source, (p.dx - f.anchor_x - left) * scale, (p.dy - f.anchor_y - top) * scale, f.width * scale, f.height * scale);
  }
  // Tint before composing, so upper unmasked parts cover lower player masks.
  if (baked.size === LIMIT) baked.delete(baked.keys().next().value);
  baked.set(key, { canvas, width: canvas.width, height: canvas.height, ox: -left * scale, oy: -top * scale, native: width });
}
export function buildingSprite(id, colour) {
  const key = `${id}:${colour & 0xffffff}`;
  if (baked.has(key)) return baked.get(key);
  if (state === 'ready' && recipes.has(id) && !pending.has(key) && pending.size < LIMIT) {
    pending.add(key);
    // Yield between compositions so cached native decodes cannot monopolize input.
    queue = queue.then(async () => {
      await new Promise(resolve => setTimeout(resolve, 0));
      return bake(id, colour, key);
    }).catch(() => {
      // A missing source keeps its generic symbol without a request/redraw loop.
      if (baked.size === LIMIT) baked.delete(baked.keys().next().value);
      baked.set(key, null);
    }).finally(() => { pending.delete(key); wake(); });
  }
  return null;
}
