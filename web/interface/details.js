// Presentation only; authoritative values and the simulation's movement rule
// arrive with the four-Hz HUD update. No canvas frame loop or terrain guesses.
const byId = id => document.getElementById(id);
const kinds = ['Berry bush', 'Tree', 'Gold deposit', 'Stone deposit'];
const icons = ['food', 'wood', 'gold', 'stone'];
const svg = name => `<svg class="icon" aria-hidden="true"><use href="/interface/icons.svg#${name}"></use></svg>`;
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
    byId('selection-portrait').innerHTML = svg(icons[values[1]]);
    byId('selected-icons').replaceChildren();
    byId('command-context').textContent = kinds[values[1]];
  }
  const cursor = ['', 'gather', 'move', 'forbidden', 'attack', 'hunt'][values[3]];
  byId('scene').dataset.commandCursor = cursor;
  const highlight = byId('resource-highlight');
  const highlighted = cursor === 'gather' && values[8] > 0;
  highlight.toggleAttribute('hidden', !highlighted);
  if (highlighted) {
    const width = values[8], height = width / 2;
    Object.assign(highlight.style, { left: `${values[4]-width/2-3}px`, top: `${values[5]-height/2-3}px`, width: `${width+6}px`, height: `${height+6}px` });
    const ellipse = highlight.firstElementChild;
    for (const [key, value] of Object.entries({ cx: width/2+3, cy: height/2+3, rx: width/2, ry: height/2 })) ellipse.setAttribute(key, value);
  }
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
