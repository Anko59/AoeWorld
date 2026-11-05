// Native bottom-menu frames. The imported DE panels are 2x art: each crop in
// the atlas is larger than the opaque frame it holds, so every panel is cut
// to its measured opaque bounds and drawn at exactly FRAME_SCALE css pixels
// per art pixel. Only the selection panel stretches, by mirroring a plain
// strip of its frame between two fixed end caps; chains stay on the caps.
export const FRAME_SCALE = .5;
// Art pixels, relative to each frame's opaque bounds. Interiors are the
// usable parchment, slate or rhombus areas; they feed the CSS layout.
export const FRAME_SPECS = {
  commands: { selector:'#commands' },
  selection: { selector:'#selection', caps:[700,322] },
  minimap: { selector:'#minimap' },
};
const MIN_FRAME = 120;
const images = new Map();
const load = page => {
  if (!images.has(page)) images.set(page, new Promise((resolve, reject) => {
    const image = new Image(); image.onload = () => resolve(image); image.onerror = reject;
    image.src = `/asset-pack/${page}`;
  }));
  return images.get(page);
};
// Smallest rectangle holding every non-transparent texel of an atlas crop.
export function opaqueBounds(data, width, height) {
  let left = width, top = height, right = -1, bottom = -1;
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    if (data[(y * width + x) * 4 + 3] === 0) continue;
    if (x < left) left = x; if (x > right) right = x;
    if (y < top) top = y; if (y > bottom) bottom = y;
  }
  return right < 0 ? null : { x:left, y:top, width:right - left + 1, height:bottom - top + 1 };
}
async function measure(art) {
  const image = await load(art.page);
  const canvas = document.createElement('canvas'); canvas.width = art.width; canvas.height = art.height;
  const context = canvas.getContext('2d', { willReadFrequently:true });
  if (!context) return null;
  context.drawImage(image, art.x, art.y, art.width, art.height, 0, 0, art.width, art.height);
  const bounds = opaqueBounds(context.getImageData(0, 0, art.width, art.height).data, art.width, art.height);
  if (!bounds || bounds.width < MIN_FRAME || bounds.height < MIN_FRAME) return null;
  return { image, canvas, bounds };
}
// CSS background that shows exactly `crop` of its atlas page, stretched to
// fill the box it is applied to.
function cropStyle(art, crop) {
  const size = `${100 * art.page_width / crop.width}% ${100 * art.page_height / crop.height}%`;
  const position = `${100 * crop.x / ((art.page_width - crop.width) || 1)}% ${100 * crop.y / ((art.page_height - crop.height) || 1)}%`;
  return { image:`url('/asset-pack/${art.page}')`, size, position };
}
function layer(node, className, box, style) {
  const part = document.createElement('i');
  part.className = `frame-art ${className}`; part.setAttribute('aria-hidden', 'true');
  Object.assign(part.style, box);
  part.style.backgroundImage = style.image; part.style.backgroundSize = style.size;
  part.style.backgroundPosition = style.position;
  if (style.repeat) part.style.backgroundRepeat = style.repeat;
  node.prepend(part);
  return part;
}
const px = value => `${value * FRAME_SCALE}px`;
// Draws the frame art for one panel and returns its natural css size, or null
// when the crop is not native menu art (the SVG chrome is then kept).
export async function frame(node, art, spec) {
  const measured = await measure(art);
  if (!measured) return null;
  const { bounds, canvas } = measured;
  const base = { x:art.x + bounds.x, y:art.y + bounds.y, width:bounds.width, height:bounds.height };
  node.querySelectorAll(':scope>.frame-art').forEach(part => part.remove());
  const natural = { width:bounds.width * FRAME_SCALE, height:bounds.height * FRAME_SCALE };
  node.style.setProperty('--frame-width', `${natural.width}px`);
  node.style.setProperty('--frame-height', `${natural.height}px`);
  if (!spec.caps || spec.caps[0] + spec.caps[1] >= bounds.width) {
    layer(node, 'frame-whole', { inset:'0' }, cropStyle(art, base));
    return { ...natural, bounds };
  }
  const [left, right] = spec.caps, middle = bounds.width - left - right;
  // The middle strip is mirrored so a tiled run never shows a seam.
  const tile = document.createElement('canvas'); tile.width = middle * 2; tile.height = bounds.height;
  const context = tile.getContext('2d');
  if (!context) return null;
  context.drawImage(canvas, bounds.x + left, bounds.y, middle, bounds.height, 0, 0, middle, bounds.height);
  context.translate(middle * 2, 0); context.scale(-1, 1);
  context.drawImage(canvas, bounds.x + left, bounds.y, middle, bounds.height, 0, 0, middle, bounds.height);
  const blob = await new Promise(resolve => tile.toBlob(resolve));
  if (!blob) return null;
  const old = node.dataset.frameTile; if (old) URL.revokeObjectURL(old);
  const url = URL.createObjectURL(blob); node.dataset.frameTile = url;
  layer(node, 'frame-mid', { left:px(left), right:px(right), top:'0', bottom:'0' },
    { image:`url('${url}')`, size:`${px(middle * 2)} 100%`, position:'0 0', repeat:'repeat-x' });
  layer(node, 'frame-left', { left:'0', top:'0', bottom:'0', width:px(left) }, cropStyle(art, { ...base, width:left }));
  layer(node, 'frame-right', { right:'0', top:'0', bottom:'0', width:px(right) },
    cropStyle(art, { ...base, x:base.x + bounds.width - right, width:right }));
  node.style.setProperty('--frame-min-width', `${(left + right) * FRAME_SCALE}px`);
  return { ...natural, bounds };
}
// Range sliders paint their filled part from --fill (0..100%).
export const fillRange = input => input.style.setProperty('--fill', `${100 * (input.value - (input.min || 0)) / ((input.max || 100) - (input.min || 0))}%`);
