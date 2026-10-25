// The strategic map is one bounded raster per map; it never requests detailed world chunks.
import { MAX_MINIMAP_BYTES, decodeMinimap, minimapWorldPoint } from '/minimap.js';
import { initFogMap } from '/navigation/fog.js';

const MAX_NUMBER_TEXT = 64;

// Preserve literal values and last-key-wins semantics of existing camera links.
export function parseCameraFragment(hash) {
  const values = new Float64Array([NaN, NaN, NaN, 0]);
  const source = hash.replace(/^#+/, '').split('&').filter(part => part.includes('=')).join('&');
  // The prefix prevents URLSearchParams from stripping a literal leading '?'.
  const params = new URLSearchParams('&' + source.replaceAll('%', '%25').replaceAll('+', '%2B'));
  for (const [key, text] of params) {
    const index = ['x', 'y', 'zoom'].indexOf(key);
    if (index !== -1) {
      // Bounded and unambiguous: a crafted run of digits cannot backtrack.
      const number = text.length <= MAX_NUMBER_TEXT && text.trim() === text
        && /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/.test(text) ? Number(text) : NaN;
      values[index] = Number.isFinite(number)
        ? (index === 2 ? Math.max(0.25, Math.min(3, number)) : number) : NaN;
    } else if (key === 'renderer') {
      values[3] = Math.max(0, ['', 'webgpu', 'webgl2', 'canvas'].indexOf(text));
    }
  }
  return values;
}

export function initNavigation() {
  const panel = document.getElementById('minimap');
  const svg = document.getElementById('minimap-map');
  const toggle = document.getElementById('overview-toggle');
  const title = document.getElementById('minimap-title');
  const image = document.getElementById('minimap-terrain');
  const camera = document.getElementById('minimap-camera');
  const primary = document.getElementById('minimap-primary');
  const viewport = document.getElementById('minimap-viewport');
  const fog = initFogMap(svg);
  const keys = new Set();
  const groups = Array.from({ length: 10 }, () => new Set());
  const groupPositions = new Map();
  let groupTap = null;
  const output = new Float64Array([0, 0, NaN, NaN]);
  let world = [1, 1];
  let center = [0, 0];
  let pending = [NaN, NaN];
  let expanded = false;
  let dragging = null;
  let mapHash = null;
  let request = null;
  let retryTimer;
  let samples = 0;
  let panImpulse = [0, 0];

  function rasterState(value) {
    if (panel.dataset.raster !== value) panel.dataset.raster = value;
  }

  function drawRaster(url, signal) {
    return new Promise((resolve, reject) => {
      function finish(error) {
        image.removeEventListener('load', loaded);
        image.removeEventListener('error', failed);
        signal.removeEventListener('abort', aborted);
        if (error) reject(error); else resolve();
      }
      const loaded = () => finish();
      const failed = () => finish(new Error('Minimap image could not be drawn'));
      const aborted = () => finish(new DOMException('Minimap request aborted', 'AbortError'));
      if (signal.aborted) { aborted(); return; }
      image.addEventListener('load', loaded);
      image.addEventListener('error', failed);
      signal.addEventListener('abort', aborted, { once: true });
      image.setAttribute('href', url);
    });
  }

  function overview(value) {
    expanded = value;
    panel.classList.toggle('expanded', value);
    toggle.setAttribute('aria-expanded', String(value));
    toggle.setAttribute('aria-label', value ? 'Close whole map' : 'Show whole map');
    panel.dataset.rasterSamples = String(samples);
    title.textContent = `${value ? 'Whole map' : 'Minimap'} · ${samples ? 'terrain' : 'schematic'}`;
    if (value) svg.focus();
  }
  toggle.addEventListener('click', () => overview(!expanded));
  document.getElementById('minimap-close').addEventListener('click', () => overview(false));

  function editable(target) {
    return target instanceof Element && !!target.closest('input,textarea,select,[contenteditable]');
  }
  function direction(key) {
    return { ArrowLeft: [-1, 0], ArrowRight: [1, 0],
      ArrowUp: [0, -1], ArrowDown: [0, 1] }[key];
  }
  document.addEventListener('keydown', event => {
    if (document.querySelector("dialog[open]") || editable(event.target) || event.ctrlKey || event.metaKey || event.altKey) return;
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
    // SVG uses a square meet viewport, including letterboxing when expanded.
    pending = minimapWorldPoint(svg.getBoundingClientRect(), event.clientX, event.clientY, world) ?? pending;
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
    clearTimeout(retryTimer);
    fog(false, []);
    panel.dataset.mapHash = hash;
    rasterState('pending');
    stopPanning();
    pending = [NaN, NaN];
    request?.abort();
    request = null;
    image.removeAttribute('href');
    samples = 0;
    overview(expanded);
    if (hash) await loadRaster(hash);
  }

  async function loadRaster(hash) {
    // Retry only the raster: a transient failure must retain camera and fog.
    rasterState('pending');
    const controller = new AbortController();
    request = controller;
    const timeout = setTimeout(() => controller.abort(), 45000);
    try {
      const response = await fetch(`/maps/${hash}/minimap`, { signal: controller.signal });
      if (!response.ok || Number(response.headers.get('content-length') ?? 0) > MAX_MINIMAP_BYTES) {
        throw new Error('Minimap raster request failed or exceeded its byte limit');
      }
      const raster = decodeMinimap(await response.arrayBuffer());
      if (mapHash !== hash || request !== controller) return;
      if (!raster) throw new Error('Invalid minimap raster');
      const canvas = document.createElement('canvas');
      canvas.width = canvas.height = raster.samples;
      const context = canvas.getContext('2d');
      if (!context) throw new Error('Minimap drawing context unavailable');
      context.putImageData(new ImageData(raster.rgba, raster.samples, raster.samples), 0, 0);
      await drawRaster(canvas.toDataURL(), controller.signal);
      if (mapHash !== hash || request !== controller) return;
      samples = raster.samples;
      overview(expanded);
      rasterState('ready');
    } catch (error) {
      if (mapHash === hash && request === controller) {
        image.removeAttribute('href');
        samples = 0;
        overview(expanded);
        rasterState('failed');
        retryTimer = setTimeout(() => {
          if (mapHash === hash && request === null) void loadRaster(hash);
        }, 5000);
      }
      if (error.name !== 'AbortError') console.warn('Minimap terrain unavailable; using schematic', error);
    } finally {
      clearTimeout(timeout);
      if (request === controller) request = null;
    }
  }

  function groupsUpdate(positions, selected = []) {
    const tracked = new Set([...groups.flatMap(group => [...group]), ...selected]);
    for (let i = 0; i < positions.length; i += 3) {
      const [id, x, y] = positions.subarray(i, i + 3);
      if (!tracked.has(id)) continue;
      if (Number.isFinite(x) && Number.isFinite(y)) groupPositions.set(id, [x, y]);
      else {
        groupPositions.delete(id);
        groups.forEach(group => group.delete(id));
      }
    }
  }

  window.aoeNavigation = {
    clearGroups() {
      groups.forEach(group => group.clear());
      groupTap = null;
      groupPositions.clear();
    },
    groupMembers: () => groups.reduce((count, group) => count + group.size, 0),
    groupsUpdate,
    group(action, selected, positions) {
      groupsUpdate(positions, selected);
      const index = action % 10;
      const operation = Math.floor(action / 10);
      if (operation >= 2) {
        if (operation === 2) groups[index].clear();
        for (const id of selected) if (groupPositions.has(id)) groups[index].add(id);
        const tracked = new Set(groups.flatMap(group => [...group]));
        for (const id of groupPositions.keys()) if (!tracked.has(id)) groupPositions.delete(id);
        groupTap = null;
        return [];
      }
      const members = [...groups[index]].sort((a, b) => a - b);
      const now = performance.now();
      if (members.length && groupTap?.index === index && now - groupTap.time <= 350) {
        const sum = members.reduce((sum, id) => {
          const p = groupPositions.get(id);
          return [sum[0] + p[0], sum[1] + p[1]];
        }, [0, 0]);
        pending = sum.map(value => value / members.length);
      }
      groupTap = { index, time: now };
      return members;
    },
    fog,
    overview: () => expanded,
    fragment: () => parseCameraFragment(location.hash),
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
      panel.dataset.zoom = String(state[8]);
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
