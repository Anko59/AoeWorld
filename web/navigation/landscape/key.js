export function initMapKey() {
  const key = document.createElement('div');
  key.id = 'strategic-map-key'; key.hidden = true;
  key.setAttribute('aria-label','Map legend and distance scale');
  key.innerHTML = '<div><span class="forest">Forests</span><span class="fields">Fields</span><span class="water">Water</span><span class="holdings">Your holdings</span></div><div class="map-scale"><i></i><b></b></div>';
  document.body.append(key);
  let previous = '';
  return {
    show(value) { key.hidden = !value; },
    update(zoom, scale) {
      const pixels = Math.hypot(64,32)*zoom*scale;
      if (!(pixels > 0) || !Number.isFinite(pixels)) return;
      const power = 10**Math.floor(Math.log10(90/pixels));
      const span = [1,2,5,10].map(n=>n*power).find(n=>n*pixels>=55) ?? power;
      const next = `${span},${pixels}`;
      if (next === previous) return;
      previous = next;
      key.querySelector('i').style.width = `${span*pixels}px`;
      key.querySelector('b').textContent = `${span.toLocaleString()} tiles`;
    },
  };
}
