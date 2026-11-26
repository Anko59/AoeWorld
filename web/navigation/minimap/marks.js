// Solid marks of the minimap's actors: whole cells joined into runs, an outline
// per owner and a keep for each town. Work is proportional to the received
// actors and the occupied cells, never to the world. The cells are anchored to
// the land, so a pan of the minimap moves the layer by whole px and neither
// regroups an actor nor splits a town.
const INK = '#0d0a06', WHITE = [255, 255, 255], BLACK = [0, 0, 0];
const LARGE = 400;     // the whole-map overview
const REACH = 2;       // cells between two buildings of one town
const TOWN = 6;        // buildings of a town
const GLYPH_PX = 14;   // a wider town shows its blocks too
const SPREAD_PX = 32;  // and a wider one only its blocks
const NAMES = ['d', 'fill', 'stroke', 'stroke-width', 'stroke-opacity', 'data-town'];

const f = value => String(Math.round(value * 100) / 100);
const rgb = colour => [(colour >> 16) & 255, (colour >> 8) & 255, colour & 255];
const mix = (from, to, part) => `rgb(${from.map((value, at) => Math.round(value + (to[at] - value) * part)).join(',')})`;

const ahead = (a, b) => a.own !== b.own ? a.own : a.count !== b.count ? a.count > b.count : a.owner < b.owner;

// The grid of cells from the projected origin of the world (`origin`, in px of
// the map) and the part of it near the map. A town over the border counts whole.
function grid(world, view, { width, height }, cell) {
  const across = view.zoom / world[0] / 2, down = view.zoom / world[1] / 2;
  const origin = [(.5 - view.center[0] * across + view.center[1] * down) * width,
    (.5 - view.center[0] * across - view.center[1] * down) * height];
  const [left, top] = origin.map(value => Math.floor((-SPREAD_PX - value) / cell));
  return { cell, across, down, origin, left, top, width, height,
    columns: Math.ceil((width + 2 * SPREAD_PX) / cell) + 1 };
}

// The winning owner of every cell, by owner: units first, then buildings. Own
// actors win a shared cell, then the majority, then the smaller owner id.
function occupied(actors, { cell, across, down, origin, left, top, width, height, columns }) {
  const layers = [new Map(), new Map()];
  for (let at = 0; at + 4 < actors.length; at += 5) {
    const kind = actors[at], owner = actors[at + 3];
    const dx = actors[at + 1] * across, dy = actors[at + 2] * down;
    // From the origin of the world: the same bits wherever the camera is.
    const x = (dx - dy) * width, y = (dx + dy) * height;
    // A unit just off the map may still show its cell or outline.
    const sx = origin[0] + x, sy = origin[1] + y, edge = kind & 1 ? SPREAD_PX : 2 * cell;
    if (!(sx >= -edge && sy >= -edge && sx < width + edge && sy < height + edge && owner >= 0)) continue;
    const place = (Math.floor(y / cell) - top) * columns + Math.floor(x / cell) - left, places = layers[kind & 1];
    let votes = places.get(place);
    if (!votes) places.set(place, votes = new Map());
    const vote = votes.get(owner);
    if (vote) vote.count++;
    else votes.set(owner, { place, owner, own: (kind & 8) !== 0, colour: actors[at + 4], count: 1 });
  }
  return layers.map(places => {
    const sides = new Map();
    for (const votes of places.values()) {
      const vote = [...votes.values()].reduce((best, next) => ahead(next, best) ? next : best);
      let side = sides.get(vote.owner);
      if (!side) sides.set(vote.owner, side = { owner: vote.owner, own: vote.own, colour: rgb(vote.colour), cells: [] });
      side.cells.push(vote);
    }
    for (const side of sides.values()) side.cells.sort((a, b) => a.place - b.place);
    return [...sides.values()].sort((a, b) => a.owner - b.owner);
  });
}

// Sorted cells, row by row, in px from the origin of the world.
function runs(cells, { cell, columns, left, top }, grow = 0) {
  let d = '';
  for (let at = 0; at < cells.length;) {
    const { place } = cells[at];
    let count = 1;
    while (cells[at + count]?.place === place + count && (place + count) % columns) count++;
    const x = (place % columns + left) * cell - grow, y = (Math.floor(place / columns) + top) * cell - grow;
    const w = count * cell + 2 * grow, h = cell + 2 * grow;
    d += `M${f(x)} ${f(y)}h${f(w)}v${f(h)}h${f(-w)}z`;
    at += count;
  }
  return d;
}

// Groups of cells at most REACH apart. `columns + REACH` keeps rows apart.
function towns(cells, { columns }) {
  const width = columns + REACH, free = new Map(), found = [];
  for (const vote of cells) {
    const column = vote.place % columns, row = Math.floor(vote.place / columns);
    free.set(row * width + column, { vote, column, row });
  }
  for (const [key, start] of free) {
    free.delete(key);
    const part = [start];
    // `part` is also the queue: it grows while its cells are visited.
    for (const { column, row } of part) {
      for (let down = -REACH; down <= REACH; down++) for (let across = -REACH; across <= REACH; across++) {
        const next = (row + down) * width + column + across, cell = free.get(next);
        if (cell && free.delete(next)) part.push(cell);
      }
    }
    found.push(part);
  }
  return found;
}

function paint(d, fill, colour, own, width, town) {
  const body = { d, fill, 'stroke-width': width, 'data-town': town };
  return own
    ? [{ d, fill: 'none', stroke: INK, 'stroke-opacity': .9, 'stroke-width': width + 2 },
      { ...body, stroke: mix(colour, WHITE, .72) }]
    : [{ ...body, stroke: INK, 'stroke-opacity': .85 }];
}

// A base and three merlons, at the middle of the town's buildings.
function keep(town, count, { cell, left, top }, { colour, own }) {
  const middle = (axis, first) => Math.round(town.reduce((sum, part) =>
    sum + (part[axis] + first + .5) * part.vote.count, 0) / count * cell);
  const x = middle('column', left), y = middle('row', top), [w, h, dip] = own ? [5, 4, 2.5] : [4, 3.5, 2];
  const step = w * .4, merlon = `h${f(step)}v${dip}h${f(step)}v${-dip}`;
  return paint(`M${x - w} ${f(y + h)}V${f(y - h)}${merlon}${merlon}h${f(step)}V${f(y + h)}z`,
    mix(colour, WHITE, 0), colour, own, own ? 2 : 2.5, count);
}

function buildings(side, cells, grow) {
  const { cell } = cells, blocks = [], keeps = [];
  for (const town of towns(side.cells, cells)) {
    const count = town.reduce((sum, part) => sum + part.vote.count, 0);
    const span = Math.max(...['column', 'row'].map(axis => {
      let least = Infinity, most = -Infinity;
      for (const part of town) { least = Math.min(least, part[axis]); most = Math.max(most, part[axis]); }
      return (most - least + 1) * cell;
    }));
    if (count < TOWN || span > GLYPH_PX) for (const part of town) blocks.push(part.vote);
    if (count >= TOWN && span <= SPREAD_PX) keeps.push(...keep(town, count, cells, side));
  }
  if (!blocks.length) return keeps;
  blocks.sort((a, b) => a.place - b.place);
  return [
    { d: runs(blocks, cells), fill: 'none', stroke: mix(side.colour, BLACK, .55), 'stroke-opacity': .6, 'stroke-width': cell * 3 },
    ...paint(runs(blocks, cells, grow), mix(side.colour, BLACK, .28), side.colour, side.own, 1.5), ...keeps,
  ];
}

export function initMarks(layer) {
  if (!layer) return () => {};
  const nodes = [];
  let shown = [];
  // Nodes keep their drawing order; only what they draw changes.
  function show(paths) {
    paths.forEach((path, index) => {
      let node = nodes[index];
      if (!node) {
        node = document.createElementNS(layer.namespaceURI, 'path');
        layer.append(node);
        nodes.push(node);
      }
      for (const name of NAMES) {
        if (shown[index]?.[name] === path[name]) continue;
        if (path[name] === undefined) node.removeAttribute(name); else node.setAttribute(name, path[name]);
      }
    });
    while (nodes.length > paths.length) nodes.pop().remove();
    shown = paths;
  }
  return (actors, world, view, frame) => {
    if (!frame) { show([]); return; }
    const cell = Math.min(frame.width, frame.height) >= LARGE || view.zoom >= 3 ? 3 : 2;
    const grow = Math.max(.5, Math.min(3, (1.5 * frame.width * view.zoom / Math.max(...world) - cell) / 2));
    const cells = grid(world, view, frame, cell);
    const [units, blocks] = occupied(actors, cells);
    // One unit is one page px. The browser puts the svg on a whole px itself,
    // so the origin of the grid is rounded within it, letterboxing included.
    const shift = frame.inset.map((value, axis) => f(Math.round(value + cells.origin[axis]) - value)).join(' ');
    const transform = `scale(${256 / frame.width} ${256 / frame.height}) translate(${shift})`;
    if (layer.getAttribute('transform') !== transform) layer.setAttribute('transform', transform);
    show([false, true].flatMap(own => [
      ...units.filter(side => side.own === own).flatMap(side =>
        paint(runs(side.cells, cells), mix(side.colour, WHITE, .1), side.colour, own, 1.5)),
      ...blocks.filter(side => side.own === own).flatMap(side => buildings(side, cells, grow)),
    ]));
  };
}
