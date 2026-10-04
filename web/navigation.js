// The strategic map is fixed-size; it never requests detailed world chunks.
export function initNavigation() {
  const panel = document.getElementById('minimap');
  const svg = document.getElementById('minimap-map');
  const toggle = document.getElementById('overview-toggle');
  const title = document.getElementById('minimap-title');
  const image = document.getElementById('minimap-terrain');
  const camera = document.getElementById('minimap-camera');
  const primary = document.getElementById('minimap-primary');
  const viewport = document.getElementById('minimap-viewport');
  const keys = new Set();
  const output = new Float64Array([0, 0, NaN, NaN]);
  let world = [1, 1];
  let center = [0, 0];
  let pending = [NaN, NaN];
  let expanded = false;
  let dragging = null;
  let mapHash = null;
  let request = null;
  let coarse = false;
  let panImpulse = [0, 0];

  function overview(value) {
    expanded = value;
    panel.classList.toggle('expanded', value);
    toggle.setAttribute('aria-expanded', String(value));
    toggle.setAttribute('aria-label', value ? 'Close whole map' : 'Show whole map');
    title.textContent = `${value ? 'Whole map' : 'Minimap'} · ${coarse ? 'coarse terrain (16×16)' : 'schematic'}`;
    if (value) svg.focus();
  }
  toggle.addEventListener('click', () => overview(!expanded));
  document.getElementById('minimap-close').addEventListener('click', () => overview(false));

  function editable(target) {
    return target instanceof Element && !!target.closest('input,textarea,select,[contenteditable]');
  }
  function direction(key) {
    return { ArrowLeft: [-1, 0], a: [-1, 0], ArrowRight: [1, 0], d: [1, 0],
      ArrowUp: [0, -1], w: [0, -1], ArrowDown: [0, 1], s: [0, 1] }[key];
  }
  document.addEventListener('keydown', event => {
    if (editable(event.target) || event.ctrlKey || event.metaKey || event.altKey) return;
    const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
    if (key === 'm' && !event.repeat) {
      event.preventDefault();
      overview(!expanded);
    } else if (key === 'Escape' && expanded) {
      overview(false);
    } else if (direction(key)) {
      event.preventDefault();
      if (!event.repeat) {
        const vector = direction(key);
        panImpulse = [panImpulse[0] + vector[0] * 16, panImpulse[1] + vector[1] * 16];
      }
      keys.add(key);
    } else if (key === 'Shift') {
      keys.add(key);
    }
  });
  document.addEventListener('keyup', event => {
    keys.delete(event.key.length === 1 ? event.key.toLowerCase() : event.key);
  });
  function stopPanning() { keys.clear(); panImpulse = [0, 0]; }
  window.addEventListener('blur', () => { stopPanning(); dragging = null; });
  document.addEventListener('visibilitychange', () => { if (document.hidden) stopPanning(); });
  document.addEventListener('focusin', event => { if (editable(event.target)) stopPanning(); });

  function target(event) {
    const rect = svg.getBoundingClientRect();
    // SVG uses a square meet viewport, including letterboxing when expanded.
    const side = Math.min(rect.width, rect.height);
    const left = rect.left + (rect.width - side) / 2;
    const top = rect.top + (rect.height - side) / 2;
    if (side <= 0) return;
    pending = [
      Math.max(0, Math.min(world[0] - 1, (event.clientX - left) / side * world[0])),
      Math.max(0, Math.min(world[1] - 1, (event.clientY - top) / side * world[1])),
    ];
  }
  svg.addEventListener('pointerdown', event => {
    if (event.button !== 0) return;
    event.preventDefault();
    dragging = event.pointerId;
    svg.setPointerCapture(event.pointerId);
    target(event);
  });
  svg.addEventListener('pointermove', event => { if (dragging === event.pointerId) target(event); });
  for (const name of ['pointerup', 'pointercancel', 'lostpointercapture']) {
    svg.addEventListener(name, () => { dragging = null; });
  }
  svg.addEventListener('keydown', event => {
    const delta = direction(event.key);
    if (!delta) return;
    event.preventDefault();
    event.stopPropagation();
    pending = [center[0] + delta[0] * world[0] / 40, center[1] + delta[1] * world[1] / 40];
  });

  function marker(element, point) {
    const valid = point.every(Number.isFinite);
    element.hidden = !valid;
    element.setAttribute('visibility', valid ? 'visible' : 'hidden');
    if (!valid) return;
    element.setAttribute('cx', String(Math.max(0, Math.min(256, point[0] / world[0] * 256))));
    element.setAttribute('cy', String(Math.max(0, Math.min(256, point[1] / world[1] * 256))));
  }

  async function changeMap(hashBytes) {
    const hash = hashBytes.length === 32
      ? Array.from(hashBytes, byte => byte.toString(16).padStart(2, '0')).join('') : '';
    if (hash === mapHash) return;
    mapHash = hash;
    panel.dataset.mapHash = hash;
    stopPanning();
    pending = [NaN, NaN];
    request?.abort();
    image.removeAttribute('href');
    coarse = false;
    overview(expanded);
    if (!hash) return;
    const controller = new AbortController();
    request = controller;
    const timeout = setTimeout(() => controller.abort(), 30000);
    try {
      const response = await fetch(`/maps/${hash}/preview`, { signal: controller.signal });
      if (!response.ok) return;
      const data = await response.json();
      if (mapHash !== hash || data.samples_per_axis !== 16 || data.cells?.length !== 256) return;
      const canvas = document.createElement('canvas');
      canvas.width = canvas.height = 16;
      const context = canvas.getContext('2d');
      if (!context) return;
      const colors = ['#668447', '#759553', '#926f46', '#bda065', '#877969', '#89a877', '#705b3f', '#887969'];
      data.cells.forEach((cell, index) => {
        context.fillStyle = cell.water !== 0 ? '#34677d' : !cell.passable ? '#9b9590' : colors[cell.material] ?? '#668447';
        context.fillRect(index % 16, Math.floor(index / 16), 1, 1);
      });
      image.setAttribute('href', canvas.toDataURL());
      coarse = true;
      overview(expanded);
    } catch (error) {
      if (error.name !== 'AbortError') console.warn('Minimap terrain unavailable; using schematic', error);
    } finally {
      clearTimeout(timeout);
      if (request === controller) request = null;
    }
  }

  window.aoeNavigation = {
    map: changeMap,
    frame(state) {
      if (state.length !== 11 || !Array.from(state.slice(0, 4)).every(Number.isFinite)) {
        output.set([0, 0, NaN, NaN]);
        return output;
      }
      world = [Math.max(1, state[0]), Math.max(1, state[1])];
      center = [state[2], state[3]];
      panel.dataset.worldWidth = String(world[0]);
      panel.dataset.worldHeight = String(world[1]);
      panel.dataset.cameraX = String(center[0]);
      panel.dataset.cameraY = String(center[1]);
      marker(camera, center);
      marker(primary, [state[4], state[5]]);
      const points = [[-1, -1], [1, -1], [1, 1], [-1, 1]].map(([x, y]) => {
        const dx = x * state[6] / (256 * state[8]);
        const dy = y * state[7] / (128 * state[8]);
        return [Math.max(0, Math.min(256, (center[0] + dx + dy) / world[0] * 256)),
          Math.max(0, Math.min(256, (center[1] - dx + dy) / world[1] * 256))].join(',');
      });
      viewport.setAttribute('points', points.join(' '));
      let delta = [0, 0];
      for (const key of keys) {
        const vector = direction(key);
        if (vector) delta = [delta[0] + vector[0], delta[1] + vector[1]];
      }
      const length = Math.hypot(...delta);
      const speed = 1600 * (keys.has('Shift') ? 3 : 1) * Math.max(0, Math.min(100, state[9])) / 1000 * state[10];
      output[0] = (length ? delta[0] / length * speed : 0) + panImpulse[0] * state[10];
      output[1] = (length ? delta[1] / length * speed : 0) + panImpulse[1] * state[10];
      panImpulse = [0, 0];
      output[2] = pending[0];
      output[3] = pending[1];
      pending = [NaN, NaN];
      return output;
    },
  };
}
