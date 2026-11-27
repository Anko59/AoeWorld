import { minimapProject } from '/minimap.js';
import { markerShape } from '/navigation/marker.js';

// In painting order. The brackets keep to whole px and the exact view does not,
// so it has its own group and no outline is rewritten by a pan.
const EXACT = ['lens', 'edge-ink', 'edge', 'spot-ink', 'spot'];
const PARTS = ['moat', 'bracket-ink', ...EXACT, 'bracket', 'echo-ink', 'echo', 'arrow-soft', 'arrow-ink', 'arrow'];
const JUMP_PX = 24, MOTION_MS = 420, STILL_MS = 700;
const SIDE = 100;     // the width of #minimap-marker before any CSS zoom
const LARGE = 400;    // the whole-map overview gets thicker strokes

const f = value => String(Math.round(value * 100) / 100);
const rectangle = (w, h) => `M${f(-w / 2)} ${f(-h / 2)}h${f(w)}v${f(h)}h${f(-w)}z`;

// The outlines around two points: `base` for the brackets, `exact` for the view.
function outlines({ kind, box, polygon, centre, size, brackets, echo, arrow }) {
  const line = (points, [x, y]) => points.map(p => `${f(p[0] - x)} ${f(p[1] - y)}`).join('L');
  const closed = (points, from) => points.length ? `M${line(points, from)}Z` : '';
  if (kind === 'arrow') {
    const head = closed(arrow, arrow[0]);
    return { base: arrow[0], exact: arrow[0], d: { 'arrow-soft': head, 'arrow-ink': head, arrow: head } };
  }
  if (kind === 'cover') {
    const edge = closed(polygon, [0, 0]);
    return { base: [0, 0], exact: [0, 0], d: { moat: edge, 'edge-ink': edge, edge } };
  }
  const base = [box.x + box.width / 2, box.y + box.height / 2], exact = centre;
  const edge = kind === 'spot' ? '' : closed(polygon, exact);
  const dot = rim => kind === 'spot' ? rectangle(Math.max(2, size[0]) + rim, Math.max(2, size[1]) + rim) : '';
  const open = corners => corners.map(points => points ? `M${line(points, base)}` : '').join('');
  const bracket = open(brackets), still = open(echo);
  return { base, exact, d: {
    moat: kind === 'frame' ? edge : rectangle(box.width, box.height),
    lens: edge, 'edge-ink': edge, edge, 'spot-ink': dot(2), spot: dot(0),
    'bracket-ink': bracket, bracket, 'echo-ink': still, echo: still,
  } };
}

// Draws the marker over the minimap: a sibling svg without a viewBox, so neither
// the map's stretch, its zoom nor its rhombus clip reaches the strokes.
export function initLocator(root) {
  if (!root) return { place() {}, draw() {} };
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');
  const group = document.createElementNS(root.namespaceURI, 'g');
  const view = document.createElementNS(root.namespaceURI, 'g');
  const parts = {};
  for (const name of PARTS) {
    const part = document.createElementNS(root.namespaceURI, 'path');
    part.setAttribute('class', `marker-${name}`);
    part.setAttribute('d', '');
    if (!EXACT.includes(name)) group.append(part);
    else {
      if (!view.parentNode) group.append(view);
      view.append(part);
    }
    parts[name] = part;
  }
  root.append(group);
  let frame = null, zoom = 1, shift = [0, 0], corner = [0, 0], measured = 0;
  let signature = '', last = null, running = [], timer;

  function attribute(node, name, value) {
    if (value === null) node.removeAttribute(name);
    else if (node.getAttribute(name) !== value) node.setAttribute(name, value);
  }
  function rest() {
    clearTimeout(timer);
    for (const animation of running.splice(0)) animation.cancel();
    attribute(root, 'data-pulse', null);
  }
  // Only the brackets move: the true view stays where it is.
  function pulse(spread) {
    rest();
    if (reduced.matches) {
      attribute(root, 'data-pulse', 'still');
      timer = setTimeout(rest, STILL_MS);
      return;
    }
    attribute(root, 'data-pulse', 'motion');
    running = [parts['bracket-ink'], parts.bracket].map(part => part.animate(
      [{ transform: `scale(${spread})`, opacity: .35 }, { transform: 'scale(1)', opacity: 1 }],
      { duration: MOTION_MS, easing: 'cubic-bezier(.165,.84,.44,1)' }));
    running[0].onfinish = rest;
  }
  // Both cameras under the same minimap view: zooming or dragging the minimap
  // moves the marker, not the camera. Nor is a move to an overlapping screen a
  // jump: on a minimap zoomed far in, every frame of a pan covers many px.
  function jumped(from, to, world, state, size) {
    const [a, b] = [from, to].map(point => minimapProject(point, world, state));
    const dx = Math.abs(a[0] - b[0]) * frame.width / 256, dy = Math.abs(a[1] - b[1]) * frame.height / 256;
    return Math.hypot(dx, dy) > JUMP_PX && (dx > size[0] || dy > size[1]);
  }
  function paint(shape) {
    const { base, exact, d } = shape ? outlines(shape) : { base: [0, 0], exact: [0, 0], d: {} };
    attribute(root, 'data-kind', shape ? shape.kind : null);
    attribute(root, 'data-cut', shape?.corners.includes(false) ? 'true' : null);
    attribute(group, 'transform', `scale(${1 / zoom}) translate(${f(shift[0] + base[0])} ${f(shift[1] + base[1])})`);
    attribute(view, 'transform', `translate(${f(exact[0] - base[0])} ${f(exact[1] - base[1])})`);
    for (const name of PARTS) attribute(parts[name], 'd', d[name] ?? '');
  }
  return {
    // Reads layout: only when the map is measured, never while drawing. The svg
    // is put on a whole page px, as browsers snap it there anyway, and undoes
    // the menu's CSS zoom (bottom-menu.css) so that its unit is one page px.
    place(next) {
      const own = root.getBoundingClientRect();
      frame = own.width > 0 ? next : null;
      measured++;
      if (!frame) return;
      zoom = own.width / SIDE;
      const whole = frame.origin.map(Math.round);
      corner = [corner[0] + (whole[0] - own.left) / zoom, corner[1] + (whole[1] - own.top) / zoom];
      shift = frame.origin.map((value, axis) => value - whole[axis]);
      root.style.left = `${corner[0]}px`;
      root.style.top = `${corner[1]}px`;
      attribute(root, 'data-size', Math.min(frame.width, frame.height) >= LARGE ? 'large' : null);
    },
    // `ground`: the four unclamped corners of the screen on the land, or null.
    draw(ground, world, state) {
      const next = [ground, world, state.zoom, state.center, measured].flat(2).join(',');
      if (next === signature) return;
      signature = next;
      const shape = frame && ground?.flat().every(Number.isFinite)
        ? markerShape({ ...frame, polygon: ground, world, view: state }) : null;
      paint(shape);
      const before = last;
      last = shape && { world: world.join(','), camera: [0, 1].map(axis => (ground[0][axis] + ground[2][axis]) / 2) };
      if (!shape) rest();
      else if (before?.world === last.world && shape.corners.includes(true)
        && jumped(before.camera, last.camera, world, state, shape.size)) pulse(shape.spread);
      // Its spread fits the place where it began: no other frame may wear it.
      else if (shape.kind === 'frame' && running.length) rest();
    },
  };
}
