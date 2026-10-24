// One strategic raster and fixed working buffers, independent of explored area.
export function initFogMap(svg) {
  const layer = document.createElementNS('http://www.w3.org/2000/svg', 'foreignObject');
  layer.id = 'minimap-fog';
  for (const name of ['width', 'height']) layer.setAttribute(name, '256');
  layer.setAttribute('pointer-events', 'none');
  document.getElementById('minimap-terrain').after(layer);
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = 256;
  canvas.style.display = 'block';
  layer.append(canvas);
  const context = canvas.getContext('2d');
  const data = context?.createImageData(256, 256);
  const previous = new Uint32Array(65536);
  let previousLength = -1, previousDisabled = null;
  layer.setAttribute('data-working-bytes', String(previous.byteLength + (data?.data.byteLength ?? 0)));
  const draw = (disabled, pixels) => {
    if (pixels.length > previous.length) return;
    if (previousDisabled === disabled && previousLength === pixels.length
      && pixels.every((value, index) => value === previous[index])) return;
    previous.set(pixels);
    previousLength = pixels.length;
    previousDisabled = disabled;
    layer.style.display = disabled ? 'none' : '';
    svg.dataset.fog = disabled ? 'disabled' : 'enabled';
    if (disabled || !context || !data) return;
    for (let at = 3; at < data.data.length; at += 4) data.data[at] = 255;
    for (const pixel of pixels) data.data[(pixel & 65535) * 4 + 3] = pixel >>> 16 === 2 ? 0 : 140;
    context.putImageData(data, 0, 0);
  };
  draw(false, []);
  return draw;
}
