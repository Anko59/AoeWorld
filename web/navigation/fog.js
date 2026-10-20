// The server supplies sparse pixels of its bounded strategic view. The raster
// below is presentation only; changing it never reveals a hidden entity.
export function initFogMap(svg) {
  const image = document.createElementNS('http://www.w3.org/2000/svg', 'image');
  image.id = 'minimap-fog';
  for (const name of ['width', 'height']) image.setAttribute(name, '256');
  image.setAttribute('pointer-events', 'none');
  document.getElementById('minimap-terrain').after(image);
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = 256;
  const context = canvas.getContext('2d');
  let previous = null, previousDisabled = null;
  const draw = (disabled, pixels) => {
    if (previousDisabled === disabled && previous?.length === pixels.length
      && pixels.every((value, index) => value === previous[index])) return;
    previous = Uint32Array.from(pixels);
    previousDisabled = disabled;
    image.style.display = disabled ? 'none' : '';
    svg.dataset.fog = disabled ? 'disabled' : 'enabled';
    if (disabled || !context) return;
    const data = context.createImageData(256, 256);
    for (let at = 3; at < data.data.length; at += 4) data.data[at] = 255;
    for (const pixel of pixels) data.data[(pixel & 65535) * 4 + 3] = pixel >>> 16 === 2 ? 0 : 140;
    context.putImageData(data, 0, 0);
    image.setAttribute('href', canvas.toDataURL());
  };
  draw(false, []);
  return draw;
}
