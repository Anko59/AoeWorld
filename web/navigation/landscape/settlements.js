import { buildingKinds, buildingNames } from '/interface/buildings.js';
import { actorColour } from '/minimap.js';
import { settlementSymbols } from '/navigation/landscape/symbols.js';

// Cache authorized plot geometry at HUD cadence; the camera draws one path
// per colour, not thousands of individual footprints on every animation frame.
export function initSettlements() {
  const layer = document.createElement('div');
  layer.id = 'strategic-settlements'; layer.hidden = true;
  document.body.append(layer);
  const nodes = new Map(), symbol = settlementSymbols();
  let hits = [];
  let rows = [], signature = '', plots = new Map(), farms = new Path2D(), furrows = new Path2D();
  let fields = 0, borders = 0;
  function clear() { plots.clear(); farms = new Path2D(); furrows = new Path2D(); fields = borders = 0; }
  return {
    update(values = []) {
      const next = Array.from(values.slice(0, 2048 * 7)).join(',');
      if (next === signature) return false;
      signature = next; rows = []; clear();
      for (let at = 0; at + 6 < Math.min(values.length, 2048 * 7); at += 7) {
        const r = Array.from(values.slice(at, at + 7));
        if (r.every(Number.isFinite) && buildingKinds[r[5]] && r[3] > r[1] && r[4] > r[2]) rows.push(r);
      }
      const rank = kind => kind === 0 ? 0 : kind === 16 ? 1 : kind === 13 ? 2 : 3;
      rows.sort((a,b) => rank(a[5]) - rank(b[5]) || a[0] - b[0]);
      for (const [,x0,y0,x1,y1,kind,colour] of rows) {
        if (kind === 5) {
          fields++; farms.rect(x0,y0,x1-x0,y1-y0);
          for (let n=1;n<4;n++) { furrows.moveTo(x0+(x1-x0)*n/4,y0); furrows.lineTo(x0+(x1-x0)*n/4,y1); }
        } else {
          const path = plots.get(colour) ?? new Path2D();
          path.rect(x0,y0,x1-x0,y1-y0); plots.set(colour,path); borders++;
        }
      }
      return true;
    },
    show(value) { layer.hidden = !value; },
    draw(context, project, ownership, width, height, scale, plane, zoom) {
      plane(); context.lineWidth = 1/(64*zoom);
      if (fields) {
        context.fillStyle = '#baa05a'; context.fill(farms);
        context.strokeStyle = '#6c5936'; context.stroke(farms); context.stroke(furrows);
      }
      if (ownership) for (const [colour,path] of plots) {
        context.fillStyle = context.strokeStyle = actorColour(colour);
        context.globalAlpha = .22; context.fill(path); context.globalAlpha = 1; context.stroke(path);
      }
      context.setTransform(1,0,0,1,0,0);
      const live = new Set(), occupied = new Set(); hits = [];
      for (const [id,x0,y0,x1,y1,kind,colour] of rows) {
        if (live.size >= 192) break;
        if ([5,7,15,20,21,22].includes(kind)) continue;
        const [x,y] = project([(x0+x1)/2,(y0+y1)/2]);
        if (x < -24 || y < -24 || x > width+24 || y > height+24) continue;
        const cell = `${Math.floor(x/24)},${Math.floor(y/24)}`;
        if (occupied.has(cell)) continue;
        occupied.add(cell); live.add(id);
        let node = nodes.get(id);
        if (!node) {
          node = document.createElement('span'); node.className = 'settlement-label';
          nodes.set(id,node); layer.append(node);
        }
        const name = buildingKinds[kind];
        if (node.dataset.kind !== name) {
          node.dataset.kind = name;
          node.textContent = buildingNames[kind];
          node.setAttribute('aria-label',buildingNames[kind]);
        }
        const icon = symbol(kind), size = (kind === 0 || kind === 16 ? 34 : 26)/scale;
        if (icon) context.drawImage(icon,x-size/2,y-size*.75,size,size);
        context.fillStyle = actorColour(colour); context.fillRect(x-size/2,y+size*.25-2/scale,size,3/scale);
        hits.push({x,y,size,world:[(x0+x1)/2,(y0+y1)/2]});
      }
      for (const [id,node] of nodes) if (!live.has(id)) { node.remove(); nodes.delete(id); }
      return { fields, borders: ownership ? borders : 0, icons: live.size };
    },
    pick(x,y) { return hits.find(p => Math.abs(x-p.x)<=p.size/2 && y>=p.y-p.size*.75 && y<=p.y+p.size*.25)?.world ?? null; },
    reset() { rows = []; hits = []; signature = ''; clear(); layer.replaceChildren(); nodes.clear(); },
  };
}
