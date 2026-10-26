import { aggregatePixels, actorColour } from '/minimap.js';

// A backend-independent, cached strategic plane. The detailed renderer keeps
// its GPU/context alive, but emits no terrain, resources or sprites at these LODs.
export function initStrategic(fog, actors) {
  const initialScene = document.getElementById('scene');
  const canvas = document.createElement('canvas');
  canvas.id = 'strategic-scene';
  canvas.hidden = true;
  canvas.width = canvas.height = 1;
  canvas.setAttribute('aria-hidden', 'true');
  const label = document.createElement('div');
  label.id = 'strategic-label';
  label.hidden = true;
  initialScene.after(canvas, label);
  const context = canvas.getContext('2d', { alpha: false });
  let terrain = null, terrainRevision = 0, previous = '', redraws = 0, picks = [], pickRadius = 0;
  function frame(state) {
    // Renderer fallback replaces a context-bound canvas during startup.
    const scene = document.getElementById('scene');
    const [worldWidth, worldHeight, cx, cy, width, height, zoom, level] = state;
    const lod = ['sprites', 'markers', 'map'][level];
    if (scene.dataset.lod !== lod) {
      scene.dataset.lod = lod;
      document.documentElement.dataset.lod = lod;
    }
    canvas.hidden = label.hidden = !level;
    const title = `${level === 2 ? 'Map view' : 'Overview'} · ${(zoom * 100).toPrecision(3)}% · click a marker to zoom in`;
    if (label.textContent !== title) label.textContent = title;
    if (!level || !context) {
      previous = ''; picks = [];
      if (canvas.width !== 1) canvas.width = canvas.height = 1;
      canvas.dataset.workingBytes = '4';
      return false;
    }
    const key = [...state, terrainRevision, actors.revision(), fog.revision].join(',');
    if (key === previous) return false;
    previous = key;
    if (canvas.width !== width) canvas.width = width;
    if (canvas.height !== height) canvas.height = height;
    // Nominal RGBA backing bytes; driver/compositor overhead is additional.
    // Terrain and fog are shared cached rasters, not duplicated per frame.
    canvas.dataset.workingBytes = String(width * height * 4);
    context.setTransform(1, 0, 0, 1, 0, 0);
    context.fillStyle = '#172516';
    context.fillRect(0, 0, width, height);
    const plane = () => context.setTransform(64 * zoom, 32 * zoom, -64 * zoom, 32 * zoom,
      width / 2 - (cx - cy) * 64 * zoom, height / 2 - (cx + cy) * 32 * zoom);
    plane();
    context.fillStyle = '#587b45';
    context.fillRect(0, 0, worldWidth, worldHeight);
    context.imageSmoothingEnabled = true;
    if (terrain) context.drawImage(terrain, 0, 0, worldWidth, worldHeight);
    const project = ([x, y]) => [width / 2 + (x - cx - y + cy) * 64 * zoom,
      height / 2 + (x - cx + y - cy) * 32 * zoom];
    // At most about 4096 marker bins or 768 counted clusters, including on
    // large/retina canvases. The input is the bounded gameplay actor stream.
    const cell = level === 2 ? Math.max(28, Math.ceil(Math.sqrt(width * height / 768)))
      : Math.max(4, Math.ceil(Math.sqrt(width * height / 4096)));
    pickRadius = Math.max(8, cell / 2);
    picks = aggregatePixels(actors.records(), project, width, height, cell);
    context.setTransform(1, 0, 0, 1, 0, 0);
    if (level === 2) {
      // Occupied settlement cells, not invented authoritative territorial borders.
      context.globalAlpha = .18;
      for (const pixel of picks) if (pixel.buildings) {
        context.fillStyle = actorColour(pixel.colour);
        context.fillRect(pixel.x - cell / 2, pixel.y - cell / 2, cell, cell);
      }
      context.globalAlpha = 1;
    }
    if (!fog.disabled) {
      plane();
      context.imageSmoothingEnabled = false;
      context.drawImage(fog.canvas, 0, 0, worldWidth, worldHeight);
    }
    context.setTransform(1, 0, 0, 1, 0, 0);
    context.font = `${Math.max(11, width / scene.clientWidth * 11)}px monospace`;
    context.textAlign = 'center';
    context.textBaseline = 'bottom';
    for (const pixel of picks) {
      context.fillStyle = actorColour(pixel.colour);
      const radius = pixel.buildings ? 4 : 2;
      context.fillRect(pixel.x - radius, pixel.y - radius, radius * 2, radius * 2);
      if (level === 2 && pixel.units > 1) {
        context.strokeStyle = '#101910';
        context.lineWidth = 3;
        context.strokeText(String(pixel.units), pixel.x, pixel.y - 5);
        context.fillText(String(pixel.units), pixel.x, pixel.y - 5);
      }
    }
    canvas.dataset.clusters = String(picks.length);
    canvas.dataset.terrainDraws = terrain ? '1' : '0';
    canvas.dataset.territory = 'settlements';
    canvas.dataset.redraws = String(++redraws);
    const scale = scene.clientWidth / width;
    canvas.dataset.picks = JSON.stringify(picks.map(p => ({ x: p.x * scale, y: p.y * scale, count: p.count })));
    return true;
  }
  function pick(x, y) {
    if (canvas.hidden) return null;
    let nearest = null, distance = Infinity;
    for (const pixel of picks) {
      const dx = x - pixel.x, dy = y - pixel.y;
      const next = dx * dx + dy * dy;
      if (Math.abs(dx) <= pickRadius && Math.abs(dy) <= pickRadius && next < distance) {
        nearest = pixel.world; distance = next;
      }
    }
    return nearest;
  }
  return { frame, pick, terrain(value) { terrain = value; terrainRevision++; } };
}
