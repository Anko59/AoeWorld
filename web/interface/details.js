import { icon } from '/interface/ui-art.js';
// Presentation only; authoritative values and the simulation's movement rule
// arrive with the four-Hz HUD update. No canvas frame loop or terrain guesses.
const byId = id => document.getElementById(id);
const kinds = ['Berry bush', 'Tree', 'Gold deposit', 'Stone deposit', 'Shore fish', 'Deep fish'];
const icons = ['food', 'wood', 'gold', 'stone', 'food', 'food'];
let markers = '';
export function updateDetails(values) {
  byId('scene').dataset.connected = String(values[0] === 1);
  if (values[1] >= 0) {
    byId('selection').dataset.count = '1';
    byId('selection').dataset.kind = 'Resource';
    byId('selection-empty').hidden = true;
    byId('selection-content').hidden = false;
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
  const cursor = ['', 'gather', 'move', 'forbidden', 'attack', 'hunt', 'gather', 'gather', 'gather', 'gather'][values[3]];
  byId('scene').dataset.commandCursor = cursor;
  byId('scene').dataset.gatherKind = ['fish', 'forage', 'chop', 'mine'][values[3] - 6] ?? '';
  // WebGPU follows the sprite silhouette; compatibility backends keep the
  // contextual gather cursor. The former ground ellipse is no longer drawn.
  byId('scene').dataset.hoverMode = byId('playground')?.dataset.hoverMode ?? 'outline';
  byId('scene').dataset.resourceHovered = String(cursor === 'gather' && values[8] > 0);
  const panel = byId('minimap');
  const world = [Number(panel.dataset.worldWidth), Number(panel.dataset.worldHeight)];
  if (!world.every(value => value > 0)) return;
  let at = 9, actors = '', resources = '';
  const point = () => [values[at+1]/world[0]*256, values[at+2]/world[1]*256];
  for (let index=0; index<values[6]; index++,at+=3) {
    const [x,y] = point();
    actors += values[at] === 1
      ? `<rect class="minimap-building" x="${x-5}" y="${y-5}" width="10" height="10"/>`
      : `<circle class="minimap-unit" cx="${x}" cy="${y}" r="3.5"/>`;
  }
  for (let index=0; index<values[7]; index++,at+=3) {
    const [x,y] = point();
    const colour = ['#ce4d59', '', '#f2cf50', '#c4c8d0'][values[at]];
    resources += `<circle cx="${x}" cy="${y}" r="2" fill="${colour}"/>`;
  }
  const targets = [];
  for (; at+3<values.length; at+=4) targets.push({kind:values[at], x:values[at+1], y:values[at+2], distance:values[at+3]});
  byId('scene').dataset.resourceTargets = JSON.stringify(targets);
  byId('scene').dataset.hudRevision = String(Number(byId('scene').dataset.hudRevision ?? 0) + 1);
  const signature = actors + '|' + resources;
  if (signature !== markers) {
    byId('minimap-entities').innerHTML = actors;
    byId('minimap-resources').innerHTML = resources;
    markers = signature;
  }
}
