import { aggregatePixels, actorColour } from '/minimap.js';
import { initLayers } from '/navigation/layers.js';
import { initTokens } from '/navigation/tokens.js';
import { initContacts } from '/navigation/contacts.js';
import { initCockpit } from '/navigation/cockpit.js';

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
  let resources = [], resourceRevision = 0;
  const knownResources = new Map(), retiredResources = new Set();
  let resourceSnapshot = false;
  let modelRevision = 0, modelSignature = '', lastState = null;
  const tokens = initTokens(), contacts = initContacts();
  const cockpit = initCockpit();
  const layers = initLayers(() => {
    modelRevision++;
    if (lastState) frame(lastState);
  });
  function frame(state) {
    // Renderer fallback replaces a context-bound canvas during startup.
    const scene = document.getElementById('scene');
    const [worldWidth, worldHeight, cx, cy, width, height, zoom, level] = state;
    const lod = ['sprites', 'markers', 'map'][level];
    tokens.show(!!level); layers.show(!!level); cockpit.show(!!level);
    if (scene.dataset.lod !== lod) {
      scene.dataset.lod = lod;
      document.documentElement.dataset.lod = lod;
    }
    if (level) canvas.hidden = label.hidden = false;
    const title = `${level === 2 ? 'Map view' : 'Overview'} · ${(zoom * 100).toPrecision(3)}% · select an army · right-click to march · clusters zoom in`;
    if (label.textContent !== title) label.textContent = title;
    if (!level || !context) {
      previous = ''; picks = [];
      return false;
    }
    const key = [...state, terrainRevision, actors.revision(), fog.revision, resourceRevision, modelRevision,
      layers.enabled.threats ? contacts.clock() : 0].join(',');
    if (key === previous) return false;
    previous = key;
    lastState = Array.from(state);
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
    let records = actors.records();
    if (!layers.enabled.armies) {
      const buildings = [];
      for (let at = 0; at + 4 < records.length; at += 5) {
        if (records[at] === 1) buildings.push(...records.slice(at, at + 5));
      }
      records = buildings;
    }
    picks = aggregatePixels(records, project, width, height, cell);
    context.setTransform(1, 0, 0, 1, 0, 0);
    if (level === 2 && layers.enabled.ownership) {
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
    canvas.dataset.threats = String(layers.enabled.threats ? contacts.draw(context, project) : 0);
    const scale = scene.clientWidth / width;
    tokens.draw(context, project, layers.enabled, scale, width, height);
    // Deposits remain legible after sprite drawing stops and above clusters.
    // Rust sends only explored, non-forest resources; forests live in terrain.
    const colours = ['#e85b8a', '#3c8f4a', '#f2cf50', '#c4c8d0', '#75c8f0', '#75c8f0'];
    const radius = level === 2 ? 3 : 2;
    for (const resource of layers.enabled.resources ? resources : []) {
      const [x, y] = project(resource.world);
      context.fillStyle = '#101910';
      context.fillRect(x - radius - 1, y - radius - 1, radius * 2 + 2, radius * 2 + 2);
      context.fillStyle = colours[resource.kind] ?? '#e85b8a';
      context.fillRect(x - radius, y - radius, radius * 2, radius * 2);
    }
    canvas.dataset.resources = String(resources.length);
    canvas.dataset.clusters = String(picks.length);
    canvas.dataset.terrainDraws = terrain ? '1' : '0';
    canvas.dataset.territory = 'settlements';
    canvas.dataset.redraws = String(++redraws);
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
  function present() {
    canvas.hidden = label.hidden = true;
    tokens.show(false); layers.show(false); cockpit.show(false);
    if (canvas.width !== 1) canvas.width = canvas.height = 1;
    canvas.dataset.workingBytes = '4';
  }
  function update(a, e, b, c = []) {
      contacts.update(c);
      const next = [a, e, b, c].map(v => Array.from(v).join(',')).join(';');
      if (next === modelSignature) return;
      modelSignature = next; tokens.update(a, e, b); modelRevision++;
  }
  return { frame, pick, present, entity: tokens.entity, update, cockpit: cockpit.update,
    // All arrays are the bounded, authorized HUD stream. Aggregate jobs once
    // at HUD cadence, rather than retaining Rust map implementations in WASM.
    receive(a, castles, b, c, workers, orders) {
      const jobs = new Map(), homes = new Map(), destinations = new Map();
      for (let at = 0; at + 4 < workers.length; at += 5) {
        const [id, manager, job, idle, home] = workers.slice(at, at + 5);
        if (home >= 0) homes.set(home, id);
        if (manager < 0) continue;
        const counts = jobs.get(manager) ?? [0, 0, 0, 0, 0];
        if (job >= 0 && job < 4) counts[job]++;
        else if (idle) counts[4]++;
        jobs.set(manager, counts);
      }
      for (let at = 0; at + 2 < orders.length; at += 3) destinations.set(orders[at], [orders[at + 1], orders[at + 2]]);
      const armies = Array.from(a), economy = [];
      for (let at = 0; at + 14 < armies.length; at += 15) {
        const target = armies[at + 3] ? destinations.get(armies[at]) : null;
        armies[at + 8] = target?.[0] ?? NaN; armies[at + 9] = target?.[1] ?? NaN;
      }
      for (let at = 0; at + 7 < castles.length; at += 8) {
        const [id, x, y, selected, garrison, managed, residents, wait] = castles.slice(at, at + 8);
        const vassal = garrison >= 0 ? garrison : homes.get(id);
        if (vassal === undefined) continue;
        economy.push(id, vassal, x, y, selected, managed, residents, wait, ...(jobs.get(vassal) ?? [0, 0, 0, 0, 0]));
      }
      update(armies, economy, b, c);
    },
    reset() {
      cockpit.reset();
      tokens.update([], [], []); contacts.reset(); resources = [];
      knownResources.clear(); retiredResources.clear(); resourceSnapshot = false;
      modelSignature = ''; modelRevision++; previous = '';
    },
    terrain(value) { terrain = value; terrainRevision++; },
    resources(values) {
      if (resourceSnapshot || typeof values?.length !== 'number') return;
      let changed = false;
      for (let at = 0; at + 4 < values.length; at += 5) {
        const kind = values[at], x = values[at + 1], y = values[at + 2], low = values[at + 3], high = values[at + 4];
        if (![kind, x, y, low, high].every(Number.isFinite)) continue;
        const key = `${high}:${low}`, old = knownResources.get(key);
        if (!old || old.kind !== kind || old.world[0] !== x || old.world[1] !== y) changed = true;
        knownResources.delete(key);
        knownResources.set(key, { kind, world: [x, y] });
      }
      while (knownResources.size > 2048) { knownResources.delete(knownResources.keys().next().value); changed = true; }
      if (changed) { resources = [...knownResources.values()]; resourceRevision++; previous = ''; }
    },
    depleted(ids, begin, complete) {
      if (begin) { retiredResources.clear(); resourceSnapshot = !complete; }
      for (let at = 0; at + 1 < ids.length; at += 2) {
        const key = `${ids[at + 1]}:${ids[at]}`;
        if (knownResources.has(key)) retiredResources.add(key);
      }
      if (!complete) return;
      let changed = false;
      for (const key of retiredResources) if (knownResources.delete(key)) changed = true;
      retiredResources.clear(); resourceSnapshot = false;
      if (changed) { resources = [...knownResources.values()]; resourceRevision++; previous = ''; }
    } };
}
