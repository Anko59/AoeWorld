import { set, attribute } from '/interface/dom/update.js';
import { icon } from '/interface/ui-art.js';
import { transportHint } from '/interface/navy/indicators.js';
// Presentation only; authoritative values and the simulation's movement rule
// arrive with the four-Hz HUD update. No canvas frame loop or terrain guesses.
const byId = id => document.getElementById(id);
const kinds = ['Berry bush', 'Tree', 'Gold deposit', 'Stone deposit', 'Shore fish', 'Deep fish'];
const icons = ['food', 'wood', 'gold', 'stone', 'food', 'food'];
let markers = '';
let waypointLayer, waypointFrame = 0;
const waypointUnits = new Map();
const waypointSharedLines = new Map();
const waypointMarkers = new Map();
const svgNode = name => document.createElementNS('http://www.w3.org/2000/svg', name);
const waypointOrders = ['move', 'attack-move'];
const waypointShapes = [
  'M0 0V-30h16l-4 6 4 6H0',
  'M0 0V-9M-12-34H12V-23Q12-14 0-9Q-12-14-12-23Z',
];
window.aoeWaypoints = values => {
  if (!values.length && !waypointUnits.size && !waypointSharedLines.size) return;
  if (!waypointLayer) {
    waypointLayer = svgNode('svg');
    waypointLayer.id = 'waypoint-markers';
    waypointLayer.setAttribute('aria-hidden', 'true');
    document.body.append(waypointLayer);
  }
  const frame = ++waypointFrame, width = innerWidth, height = innerHeight;
  const shared = new Map();
  let previousX, previousY;
  for (let i = 0; i + 5 < values.length; i += 6) {
    const id = values[i], index = values[i+1], sharedOrder = values[i+5];
    const key = sharedOrder || `${id}/${index}`;
    let marker = waypointMarkers.get(key);
    const x = marker?.frame === frame ? marker.x : values[i+2];
    const y = marker?.frame === frame ? marker.y : values[i+3];
    const order = values[i+4] === 1 ? 1 : 0;
    if (!Number.isFinite(id) || !Number.isFinite(index) || !Number.isFinite(x) || !Number.isFinite(y)) continue;
    const fromX = previousX, fromY = previousY;
    previousX = x; previousY = y;
    if (index === 0) continue;
    if (sharedOrder) {
      const leg = shared.get(key);
      if (leg) {
        leg.x += fromX;
        leg.y += fromY;
        leg.count++;
      } else {
        shared.set(key, { x: fromX, y: fromY, count: 1, order });
      }
    } else {
      let unit = waypointUnits.get(id);
      if (!unit) {
        const firstFlag = waypointLayer.querySelector('g');
        const lines = waypointOrders.map(kind => {
          const line = svgNode('path');
          line.dataset.order = kind;
          waypointLayer.insertBefore(line, firstFlag);
          return line;
        });
        unit = { lines, frame: 0, paths: ['', ''] };
        waypointUnits.set(id, unit);
      }
      if (unit.frame !== frame) {
        unit.frame = frame;
        unit.paths[0] = unit.paths[1] = '';
      }
      // Restart the dash pattern at every leg so overlapping routes keep their gaps.
      unit.paths[order] += `M${fromX},${fromY}L${x},${y}`;
    }
    if (!marker) {
      const node = svgNode('g');
      node.dataset.unit = String(id);
      node.innerHTML = '<ellipse rx="10" ry="5"/><path/><text y="-21"></text>';
      const [, glyph, number] = node.children;
      waypointLayer.append(node);
      marker = { node, glyph, number, x: NaN, y: NaN, visible: null, frame: 0 };
      waypointMarkers.set(key, marker);
    }
    // Keep the queue's numbering until its last member has passed an earlier leg.
    marker.index = marker.frame === frame ? Math.max(marker.index, index) : index;
    marker.frame = frame;
    if (marker.order !== order) {
      marker.node.dataset.order = waypointOrders[order];
      marker.glyph.setAttribute('d', waypointShapes[order]);
      marker.number.setAttribute('x', order ? '0' : '5');
      marker.order = order;
    }
    if (marker.x !== x || marker.y !== y) {
      marker.node.setAttribute('transform', `translate(${x} ${y})`);
      marker.x = x; marker.y = y;
    }
    const visible = x >= -32 && x <= width + 32 && y >= -40 && y <= height + 40;
    if (marker.visible !== visible) {
      marker.node.setAttribute('visibility', visible ? 'visible' : 'hidden');
      marker.visible = visible;
    }
  }
  for (const [key, leg] of shared) {
    let line = waypointSharedLines.get(key);
    if (!line) {
      line = svgNode('path');
      waypointLayer.insertBefore(line, waypointLayer.querySelector('g'));
      waypointSharedLines.set(key, line);
    }
    const marker = waypointMarkers.get(key);
    const path = `M${leg.x / leg.count},${leg.y / leg.count}L${marker.x},${marker.y}`;
    const order = waypointOrders[leg.order];
    if (line.dataset.order !== order) line.dataset.order = order;
    if (line.getAttribute('d') !== path) line.setAttribute('d', path);
  }
  for (const [key, line] of waypointSharedLines) {
    if (shared.has(key)) continue;
    line.remove();
    waypointSharedLines.delete(key);
  }
  for (const [id, unit] of waypointUnits) {
    if (unit.frame !== frame) {
      for (const line of unit.lines) line.remove();
      waypointUnits.delete(id);
    } else {
      for (let order = 0; order < unit.lines.length; order++) {
        const line = unit.lines[order], path = unit.paths[order];
        if (line.getAttribute('d') !== path) line.setAttribute('d', path);
      }
    }
  }
  for (const [key, marker] of waypointMarkers) {
    if (marker.frame !== frame) {
      marker.node.remove(); waypointMarkers.delete(key);
    } else if (marker.shownIndex !== marker.index) {
      marker.shownIndex = marker.index;
      marker.node.dataset.waypoint = String(marker.index);
      marker.number.textContent = String(marker.index);
    }
  }
};
// A brief ring where an attack-move was ordered; it removes itself.
window.aoeOrderPing = (x, y) => {
  if (!Number.isFinite(x) || !Number.isFinite(y)) return;
  const ring = document.createElement('div');
  ring.className = 'order-ping';
  ring.dataset.orderPing = 'attack-move';
  ring.style.left = `${x}px`;
  ring.style.top = `${y}px`;
  document.body.append(ring);
  setTimeout(() => ring.remove(), 900);
};
export function updateDetails(values) {
  attribute(byId('scene'), 'data-connected', String(values[0] === 1));
  if (values[1] >= 0) {
    attribute(byId('selection'), 'data-count', '1');
    attribute(byId('selection'), 'data-kind', 'Resource');
    set(byId('selection-empty'), 'hidden', true);
    set(byId('selection-content'), 'hidden', false);
    set(byId('selection-owner'), 'textContent', 'Natural resource');
    set(byId('selection-name'), 'textContent', kinds[values[1]]);
    set(byId('selection-task'), 'textContent', values[2] ? 'Natural resource' : 'Exhausted');
    set(byId('selection-load'), 'textContent', `${values[2]} ${icons[values[1]]} remaining`);
    const portrait=byId('selection-portrait'), key=`resource-${values[1]}`;
    if (portrait.dataset.entityIcon!==key) {
      portrait.dataset.entityIcon=key;
      portrait.innerHTML=icon('units',[59,349,66,102,53,53][values[1]],icons[values[1]]);
    }
    set(byId('selection-health'), 'hidden', true);
    if (byId('selected-icons').firstChild) byId('selected-icons').replaceChildren();
    set(byId('command-context'), 'textContent', kinds[values[1]]);
  }
  const cursor = ['', 'gather', 'move', 'forbidden', 'attack', 'hunt', 'gather', 'gather', 'gather', 'gather', 'build', 'garrison', 'board'][values[3]];
  attribute(byId('scene'), 'data-command-cursor', cursor);
  transportHint(cursor === 'board');
  attribute(byId('scene'), 'data-gather-kind', ['fish', 'forage', 'chop', 'mine'][values[3] - 6] ?? '');
  // GPU backends follow the sprite silhouette; Canvas keeps the
  // contextual gather cursor. The former ground ellipse is no longer drawn.
  attribute(byId('scene'), 'data-hover-mode', byId('playground')?.dataset.hoverMode ?? 'outline');
  attribute(byId('scene'), 'data-resource-hovered', String(cursor === 'gather' && values[8] > 0));
  const panel = byId('minimap');
  const world = [Number(panel.dataset.worldWidth), Number(panel.dataset.worldHeight)];
  if (!world.every(value => value > 0)) return;
  window.aoeNavigation?.actors(values.subarray(9, 9 + values[6] * 5));
  let at = 9 + values[6] * 5, resources = '';
  window.aoeNavigation?.resources(values.subarray(at, at + values[7] * 5));
  const point = () => [values[at+1]/world[0]*256, values[at+2]/world[1]*256];
  for (let index=0; index<values[7]; index++,at+=5) {
    if (values[at] === 1) continue;
    const [x,y] = point();
    const colour = ['#ce4d59', '', '#f2cf50', '#c4c8d0', '#75c8f0', '#75c8f0'][values[at]];
    resources += `<circle cx="${x}" cy="${y}" r="${Math.min(2, 128 / Math.max(...world))}" fill="${colour}"/>`;
  }
  const targets = [];
  for (; at+3<values.length; at+=4) targets.push({kind:values[at], x:values[at+1], y:values[at+2], distance:values[at+3]});
  attribute(byId('scene'), 'data-resource-targets', JSON.stringify(targets));
  attribute(byId('scene'), 'data-hud-revision', String(Number(byId('scene').dataset.hudRevision ?? 0) + 1));
  const signature = resources;
  if (signature !== markers) {
    byId('minimap-resources').innerHTML = resources;
    markers = signature;
  }
}
