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
const svgNode = name => document.createElementNS('http://www.w3.org/2000/svg', name);
window.aoeWaypoints = values => {
  if (!values.length && !waypointUnits.size) return;
  if (!waypointLayer) {
    waypointLayer = svgNode('svg');
    waypointLayer.id = 'waypoint-markers';
    waypointLayer.setAttribute('aria-hidden', 'true');
    document.body.append(waypointLayer);
  }
  const frame = ++waypointFrame, width = innerWidth, height = innerHeight;
  for (let i = 0; i + 3 < values.length; i += 4) {
    const id = values[i], index = values[i+1], x = values[i+2], y = values[i+3];
    if (!Number.isFinite(id) || !Number.isFinite(index) || !Number.isFinite(x) || !Number.isFinite(y)) continue;
    let unit = waypointUnits.get(id);
    if (!unit) {
      unit = { line: svgNode('polyline'), markers: new Map(), frame: 0, points: '' };
      waypointUnits.set(id, unit);
      // Keep paths beneath flags without moving any retained marker nodes.
      waypointLayer.insertBefore(unit.line, waypointLayer.querySelector('g'));
    }
    if (unit.frame !== frame) { unit.frame = frame; unit.points = ''; }
    unit.points += `${unit.points ? ' ' : ''}${x},${y}`;
    let marker = unit.markers.get(index);
    if (!marker) {
      const node = svgNode('g');
      node.dataset.unit = String(id);
      node.dataset.waypoint = String(index);
      node.innerHTML = '<ellipse rx="10" ry="5"/><path d="M0 0V-30h16l-4 6 4 6H0"/><text x="5" y="-21"></text>';
      node.lastElementChild.textContent = String(index);
      waypointLayer.append(node);
      marker = { node, x: NaN, y: NaN, visible: null, frame: 0 };
      unit.markers.set(index, marker);
    }
    marker.frame = frame;
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
  for (const [id, unit] of waypointUnits) {
    for (const [index, marker] of unit.markers) {
      if (marker.frame !== frame) { marker.node.remove(); unit.markers.delete(index); }
    }
    if (unit.frame !== frame) { unit.line.remove(); waypointUnits.delete(id); }
    else if (unit.line.getAttribute('points') !== unit.points) unit.line.setAttribute('points', unit.points);
  }
};
export function updateDetails(values) {
  byId('scene').dataset.connected = String(values[0] === 1);
  if (values[1] >= 0) {
    byId('selection').dataset.count = '1';
    byId('selection').dataset.kind = 'Resource';
    byId('selection-empty').hidden = true;
    byId('selection-content').hidden = false;
    byId('selection-owner').textContent = 'Natural resource';
    byId('selection-name').textContent = kinds[values[1]];
    byId('selection-task').textContent = values[2] ? 'Natural resource' : 'Exhausted';
    byId('selection-load').textContent = `${values[2]} ${icons[values[1]]} remaining`;
    const portrait=byId('selection-portrait'), key=`resource-${values[1]}`;
    if (portrait.dataset.entityIcon!==key) {
      portrait.dataset.entityIcon=key;
      portrait.innerHTML=icon('units',[59,349,66,102,53,53][values[1]],icons[values[1]]);
    }
    byId('selection-health').hidden=true;
    byId('selected-icons').replaceChildren();
    byId('command-context').textContent = kinds[values[1]];
  }
  const cursor = ['', 'gather', 'move', 'forbidden', 'attack', 'hunt', 'gather', 'gather', 'gather', 'gather', 'build', 'garrison', 'board'][values[3]];
  byId('scene').dataset.commandCursor = cursor;
  transportHint(cursor === 'board');
  byId('scene').dataset.gatherKind = ['fish', 'forage', 'chop', 'mine'][values[3] - 6] ?? '';
  // GPU backends follow the sprite silhouette; Canvas keeps the
  // contextual gather cursor. The former ground ellipse is no longer drawn.
  byId('scene').dataset.hoverMode = byId('playground')?.dataset.hoverMode ?? 'outline';
  byId('scene').dataset.resourceHovered = String(cursor === 'gather' && values[8] > 0);
  const panel = byId('minimap');
  const world = [Number(panel.dataset.worldWidth), Number(panel.dataset.worldHeight)];
  if (!world.every(value => value > 0)) return;
  window.aoeNavigation?.actors(values.subarray(9, 9 + values[6] * 5));
  let at = 9 + values[6] * 5, resources = '';
  const point = () => [values[at+1]/world[0]*256, values[at+2]/world[1]*256];
  for (let index=0; index<values[7]; index++,at+=3) {
    const [x,y] = point();
    const colour = ['#ce4d59', '', '#f2cf50', '#c4c8d0'][values[at]];
    resources += `<circle cx="${x}" cy="${y}" r="${Math.min(2, 128 / Math.max(...world))}" fill="${colour}"/>`;
  }
  const targets = [];
  for (; at+3<values.length; at+=4) targets.push({kind:values[at], x:values[at+1], y:values[at+2], distance:values[at+3]});
  byId('scene').dataset.resourceTargets = JSON.stringify(targets);
  byId('scene').dataset.hudRevision = String(Number(byId('scene').dataset.hudRevision ?? 0) + 1);
  const signature = resources;
  if (signature !== markers) {
    byId('minimap-resources').innerHTML = resources;
    markers = signature;
  }
}
