// Where the camera's view is on the minimap. A closed outline is the true view;
// open brackets only locate it, so a view of one pixel is still found. Lengths
// are page px from the top left of the drawn map: neither its stretch nor the
// minimap zoom scales them.
import { minimapProject } from '/minimap.js';

const SPOT = 4;           // a true view thinner than this is a dot
const ARROW_INSET = 12;
const COVER_INSET = 4.5;  // half a moat, so a cover stays on its map
const ECHO = 6;           // the still bracket's distance around the others
const SPREAD = 2.6;       // a pulse starts at that many times its box
const STROKE = 4.25;      // how far a bracket paints beyond its own line
const BEYOND = 4;         // and a frame's pulse beyond the map
// Tiles at the world's border and px at the map's: a camera that stands on the
// border is inside, whatever the rounding.
const EPSILON = 1e-6;

const clamp = (value, least, most) => Math.max(least, Math.min(most, value));
const even = value => Math.round(value / 2) * 2;

// The smallest brackets, from the shorter side of the drawn map.
function minimumBox(side) {
  const width = even(clamp(18 + side / 15, 26, 48));
  return [width, even(width * .68)];
}

// Cut, never clamp: a clamped corner bends the view. Each plane gives the depth
// of a point inside it.
function cut(polygon, planes) {
  for (const depth of planes) {
    const kept = [];
    polygon.forEach((a, index) => {
      const b = polygon[(index + 1) % polygon.length], from = depth(a), to = depth(b);
      if (from >= 0) kept.push(a);
      if ((from >= 0) !== (to >= 0)) {
        const part = from / (from - to);
        kept.push([a[0] + (b[0] - a[0]) * part, a[1] + (b[1] - a[1]) * part]);
      }
    });
    polygon = kept;
  }
  return polygon;
}

// Strictly inside a clockwise convex polygon, so an empty one holds nothing.
const holds = (polygon, p) => polygon.every((a, index) => {
  const b = polygon[(index + 1) % polygon.length];
  return (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]) > 0;
});

const onWorld = world => [p => p[0], p => world[0] - p[0], p => p[1], p => world[1] - p[1]];

// A stretched map shows its rhombus, a letterboxed one its whole box: zoomed
// land also fills the bars beside the square. `reach` is 1 on its border; the
// planes give px.
function visibleMap(width, height, stretch, inset) {
  const [wide, high] = [width + 2 * inset[0], height + 2 * inset[1]];
  const round = [[1, 0], [0, 1], [-1, 0], [0, -1]], aslant = [[1, 1], [-1, 1], [-1, -1], [1, -1]];
  const sides = stretch ? aslant : round;
  const share = ([x, y], p) => 2 * (x * (p[0] - width / 2) / wide + y * (p[1] - height / 2) / high);
  return {
    half: Math.min(wide, high) / 2,
    border: part => (stretch ? round : aslant).map(([x, y]) =>
      [(width + x * part * wide) / 2, (height + y * part * high) / 2]),
    reach: p => Math.max(...sides.map(side => share(side, p))),
    planes: sides.map(side => {
      const slope = 2 * Math.hypot(side[0] / wide, side[1] / high);
      return p => (1 - share(side, p)) / slope + EPSILON;
    }),
  };
}

// The brackets on the kept corners of a box that the planes hold: the end of
// one leg, the corner and the end of the other, each leg cut at the planes.
function bracketed({ x, y, width, height }, kept, planes) {
  const across = clamp(width * .3, 6, 18), down = clamp(height * .3, 5, 14);
  return [[0, 0], [1, 0], [1, 1], [0, 1]].map(([right, below], index) => {
    const corner = [x + right * width, y + below * height];
    if (!kept[index] || !planes.every(depth => depth(corner) >= 0)) return null;
    const leg = (dx, dy) => {
      const part = Math.min(1, ...planes.map(depth => {
        const from = depth(corner), to = depth([corner[0] + dx, corner[1] + dy]);
        return to < 0 ? from / (from - to) : 1;
      }));
      return [corner[0] + dx * part, corner[1] + dy * part];
    };
    return [leg(0, below ? -down : down), corner, leg(right ? -across : across, 0)];
  });
}

// How wide the brackets may start around that point and stay on the map.
function spread(point, brackets, planes) {
  let most = SPREAD;
  for (const depth of planes) {
    const from = depth(point);
    for (const p of brackets.flat()) {
      const fall = p ? from - depth(p) + STROKE : 0;
      if (fall > 0) most = Math.min(most, (from + BEYOND) / fall);
    }
  }
  return Math.max(1, most);
}

// Inside the border, pointing away from the centre of the map; its tip first.
function arrowhead(target, width, height, half, reach) {
  const dx = target[0] - width / 2, dy = target[1] - height / 2;
  const length = Math.hypot(dx, dy) || 1, ux = dx / length, uy = dy / length;
  const part = Math.max(0, 1 - ARROW_INSET / half) / (reach || 1);
  const x = width / 2 + dx * part, y = height / 2 + dy * part;
  return [[7, 0], [-5, 6], [-2, 0], [-5, -6]].map(([along, aside]) =>
    [x + ux * along - uy * aside, y + uy * along + ux * aside]);
}

// `polygon`: the four unclamped ground corners, clockwise from top left.
// `origin`: the drawn map's place on the page, where the brackets are rounded.
// `inset`: the bars beside it.
export function markerShape({ polygon, world, view, width, height, stretch, inset = [0, 0], origin = [0, 0] }) {
  const project = point => {
    const [x, y] = minimapProject(point, world, view);
    return [x * width / 256, y * height / 256];
  };
  const full = polygon.map(project), map = visibleMap(width, height, stretch, inset);
  const xs = full.map(p => p[0]), ys = full.map(p => p[1]);
  const left = Math.min(...xs), top = Math.min(...ys);
  const size = [Math.max(...xs) - left, Math.max(...ys) - top];
  const centre = [left + size[0] / 2, top + size[1] / 2];
  const least = minimumBox(Math.min(width, height));
  const land = cut(polygon, onWorld(world)).map(project);
  const large = size[0] >= least[0] && size[1] >= least[1] && land.length > 0;
  // A large view is drawn, and lives, as far as the map shows it. A view smaller
  // than its own brackets goes with its centre, the place they locate.
  const drawn = large ? cut(land, map.planes) : land;
  const gone = large ? drawn.length === 0 : map.reach(centre) > 1 + EPSILON;
  const kind = map.border(1).every(corner => holds(full, corner)) ? 'cover' : gone ? 'arrow'
    : large ? 'frame' : Math.min(...size) < SPOT ? 'spot' : 'inset';
  const [x, y] = large ? centre
    : centre.map((value, axis) => Math.round(value + origin[axis]) - origin[axis]);
  const wide = Math.max(least[0], size[0]), high = Math.max(least[1], size[1]);
  const box = { x: x - wide / 2, y: y - high / 2, width: wide, height: high };
  const planes = kind === 'frame' ? map.planes : [];
  const kept = polygon.map(p => kind === 'spot' || kind === 'inset'
    || kind === 'frame' && onWorld(world).every(depth => depth(p) >= -EPSILON));
  const brackets = bracketed(box, kept, planes);
  const inner = Math.max(.9, 1 - COVER_INSET / Math.min(...map.planes.map(depth => depth([width / 2, height / 2]))));
  return {
    kind, box, centre, size, brackets,
    polygon: kind === 'cover' ? map.border(inner) : drawn,
    corners: brackets.map(bracket => bracket !== null),
    echo: bracketed({ x: box.x - ECHO, y: box.y - ECHO, width: wide + 2 * ECHO, height: high + 2 * ECHO }, kept, planes),
    spread: spread(centre, brackets, planes),
    arrow: kind === 'arrow' ? arrowhead(centre, width, height, map.half, map.reach(centre)) : null,
  };
}
