import { aggregateActors, actorColour, minimapProject, minimapWorldPoint } from '/minimap.js';

// Terrain/fog are retained SVG images. Only their transform and occupied actor
// pixels change; neither zoom nor panning regenerates a world raster.
export function initMinimapView(svg, panel, jump) {
  const layer = document.getElementById('minimap-view');
  const entities = document.getElementById('minimap-entities');
  const indicator = document.getElementById('minimap-zoom');
  const nodes = new Map();
  let world = [1, 1], camera = [0, 0], actors = [], revision = 0;
  let zoom = 1, center = [.5, .5], follow = true, dragging = null;
  let signature = '', side = 1;
  const state = () => ({ zoom, center });
  function attribute(node, key, value) {
    value = String(value);
    if (node.getAttribute(key) !== value) node.setAttribute(key, value);
  }
  function render() {
    const bounds = svg.getBoundingClientRect();
    side = Math.max(1, Math.floor(Math.min(bounds.width, bounds.height)));
    const next = [world, center, zoom, side, revision].flat().join(',');
    if (signature === next) return;
    signature = next;
    const p = minimapProject(center, world);
    attribute(layer, 'transform', `translate(128 128) scale(${zoom}) translate(${-p[0]} ${-p[1]})`);
    panel.dataset.minimapZoom = String(zoom);
    indicator.textContent = `${Number(zoom.toFixed(1))}×`;
    const live = new Set();
    for (const pixel of aggregateActors(actors, world, state(), side)) {
      live.add(pixel.key);
      let node = nodes.get(pixel.key);
      if (!node) {
        node = document.createElementNS(svg.namespaceURI, 'rect');
        entities.append(node);
        nodes.set(pixel.key, node);
      }
      const scale = 256 / side;
      const values = { x: (pixel.x - pixel.size / 2) * scale, y: (pixel.y - pixel.size / 2) * scale,
        width: pixel.size * scale, height: pixel.size * scale, fill: actorColour(pixel.colour),
        'data-player': pixel.owner, 'data-count': pixel.count,
        'data-units': pixel.units, 'data-buildings': pixel.buildings,
        class: pixel.buildings ? 'minimap-building' : 'minimap-unit' };
      for (const [key, value] of Object.entries(values)) attribute(node, key, value);
    }
    for (const [key, node] of nodes) if (!live.has(key)) { node.remove(); nodes.delete(key); }
  }
  function reset() { zoom = 1; follow = true; update(world, camera); }
  function update(size, position) {
    world = size; camera = position;
    if (follow) center = zoom === 1 ? world.map(v => v / 2) : [...camera];
    render();
  }
  svg.addEventListener('wheel', event => {
    event.preventDefault();
    const delta = event.deltaY * (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? side : 1);
    if (!Number.isFinite(delta)) return;
    zoom = Math.max(1, Math.min(1024, zoom * Math.exp(-delta * .0015)));
    if (zoom === 1) follow = true;
    update(world, camera);
  }, { passive: false });
  svg.addEventListener('pointerdown', event => {
    if (event.button !== 0) return;
    event.preventDefault();
    dragging = { id: event.pointerId, x: event.clientX, y: event.clientY, center: [...center], moved: false };
    svg.setPointerCapture(event.pointerId);
  });
  svg.addEventListener('pointermove', event => {
    if (dragging?.id !== event.pointerId) return;
    const dx = (event.clientX - dragging.x) / side / zoom;
    const dy = (event.clientY - dragging.y) / side / zoom;
    if (Math.hypot(event.clientX - dragging.x, event.clientY - dragging.y) <= 4 && !dragging.moved) return;
    dragging.moved = true; follow = false;
    center = [dragging.center[0] - (dx + dy) * world[0], dragging.center[1] - (dy - dx) * world[1]]
      .map((v, i) => Math.max(0, Math.min(world[i] - 1, v)));
    render();
  });
  svg.addEventListener('pointerup', event => {
    if (dragging?.id !== event.pointerId) return;
    if (!dragging.moved) jump(minimapWorldPoint(svg.getBoundingClientRect(), event.clientX, event.clientY, world, state()));
    dragging = null;
    svg.releasePointerCapture(event.pointerId);
  });
  const cancel = () => { dragging = null; };
  for (const name of ['pointercancel', 'lostpointercapture']) svg.addEventListener(name, cancel);
  window.addEventListener('blur', cancel);
  indicator.addEventListener('click', reset);
  new ResizeObserver(render).observe(svg);
  return { update, reset, state, records: () => actors, revision: () => revision,
    actors(values) {
      if (values.length === actors.length && values.every((value, index) => value === actors[index])) return;
      actors = Array.from(values); revision++; render();
    } };
}
