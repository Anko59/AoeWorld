import { buildingKinds, buildingNames } from '/interface/buildings.js';
import { actorColour } from '/minimap.js';
import { settlementSymbols } from '/navigation/landscape/symbols.js';
import { buildingSprite, loadSprites } from '/navigation/landscape/sprites.js';
import { drawChips } from '/navigation/landscape/chips.js';

// Minimum on-screen width (CSS px) per building kind; true size takes over when zoomed in.
const MIN = { 0: 84, 16: 88, 17: 56, 18: 56, 19: 56, 1: 30, 2: 34, 3: 34, 4: 40, 8: 28, 14: 30 };
const RANK = { 0: 9, 16: 8, 17: 6, 18: 5, 19: 5, 6: 4, 13: 4, 10: 4, 11: 4, 9: 4, 12: 3, 8: 3, 14: 3, 4: 2, 2: 2, 3: 2, 1: 1 };

// Cache authorized plot geometry at HUD cadence; the camera draws one path
// per colour, not thousands of individual footprints on every animation frame.
export function initSettlements() {
  const layer = document.createElement('div');
  layer.id = 'strategic-settlements'; layer.hidden = true;
  document.body.append(layer);
  const nodes = new Map(), symbolFor = settlementSymbols();
  let ranked = [], placed = [];
  let hits = [];
  let rows = [], signature = '', plots = new Map(), farms = new Path2D(), furrows = new Path2D();
  let fields = 0, borders = 0;
  function clear() { plots.clear(); farms = new Path2D(); furrows = new Path2D(); fields = borders = 0; }
  return {
    update(values = []) {
      const next = Array.from(values.slice(0, 2048 * 9)).join(',');
      if (next === signature) return false;
      signature = next; rows = []; clear();
      for (let at = 0; at + 8 < Math.min(values.length, 2048 * 9); at += 9) {
        const r = Array.from(values.slice(at, at + 9));
        if (r.every(Number.isFinite) && [0, 1].includes(r[7]) && buildingKinds[r[5]] && r[3] > r[1] && r[4] > r[2]) rows.push(r);
      }
      const rank = kind => kind === 0 ? 0 : kind === 16 ? 1 : kind === 13 ? 2 : 3;
      rows.sort((a,b) => b[7] - a[7] || rank(a[5]) - rank(b[5]) || a[0] - b[0]);
      for (const [,x0,y0,x1,y1,kind,colour] of rows) {
        if (kind === 5) {
          fields++; farms.rect(x0,y0,x1-x0,y1-y0);
          for (let n=1;n<4;n++) { furrows.moveTo(x0+(x1-x0)*n/4,y0); furrows.lineTo(x0+(x1-x0)*n/4,y1); }
        } else {
          const path = plots.get(colour) ?? new Path2D();
          path.rect(x0,y0,x1-x0,y1-y0); plots.set(colour,path); borders++;
        }
      }
      ranked = rows.filter(r => ![5,7,15,20,21,22,23].includes(r[5])).sort((a, b) => (RANK[b[5]] ?? 1) - (RANK[a[5]] ?? 1) || b[7] - a[7] || a[0] - b[0]);
      return true;
    },
    show(value) { layer.hidden = !value; },
    draw(context, project, ownership, width, height, scale, plane, zoom, space) {
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
      loadSprites();
      // One strong symbol per place: highest rank first, each symbol reserves
      // its screen rectangle, and anything that would overlap folds into it as a count.
      placed = []; hits = [];
      const live = new Set();
      let sprites = 0;
      for (const row of ranked) {
        if (placed.length >= 192) break;
        const [id,x0,y0,x1,y1,kind,colour,owned,native] = row;
        const [x,y] = project([(x0+x1)/2,(y0+y1)/2]);
        const sprite = buildingSprite(native, colour);
        const minimum = MIN[kind] ?? 46;
        const css = Math.max(minimum, sprite ? sprite.native * zoom : 0);
        const w = css / scale;
        const s = sprite ? w / sprite.width : 1;
        const left = sprite ? x - sprite.ox * s : x - w / 2, top = sprite ? y - sprite.oy * s : y - w * .75;
        const h = sprite ? sprite.height * s : w;
        if (left > width || left + w < 0 || top > height || top + h < 0) continue;
        const r = [left * scale, (top + h * .1) * scale, (left + w) * scale, (top + h) * scale + 12];
        if (!space.free(...r)) {
          const at = space.owner(...r);
          if (at >= 0) placed[at]?.fold(kind);
          continue;
        }
        const index = placed.length;
        space.take(...r, index);
        placed.push({ id, x, y, w, h, left, top, kind, colour, owned, buildings: 1, soldiers: 0, workers: 0, world: [(x0+x1)/2,(y0+y1)/2],
          fold() { this.buildings++; } });
        const tint = actorColour(colour), base = w * .42;
        context.fillStyle = '#0006'; context.beginPath(); context.ellipse(x, y + w * .04, base * 1.05, base * .52, 0, 0, Math.PI * 2); context.fill();
        context.beginPath(); context.ellipse(x, y, base, base * .5, 0, 0, Math.PI * 2);
        context.globalAlpha = owned ? .62 : .38; context.fillStyle = tint; context.fill(); context.globalAlpha = 1;
        context.lineWidth = owned ? 2 / scale : 1 / scale; context.strokeStyle = owned ? '#fff8d6' : '#2b2116'; context.stroke();
        if (sprite) { context.drawImage(sprite.canvas, left, top, w, h); sprites++; }
        else { const icon = symbolFor(kind); if (icon) context.drawImage(icon, x - w / 2, y - w * .75, w, w); }
        // Accessible labels are retained for every drawn place (at most 192).
        let node = nodes.get(id);
        if (!node) { node = document.createElement('span'); node.className = 'settlement-label'; nodes.set(id,node); layer.append(node); }
        const name = buildingKinds[kind], flag = String(!!owned);
        if (node.dataset.kind !== name || node.dataset.owned !== flag) {
          node.dataset.owned = flag; node.dataset.kind = name; node.textContent = buildingNames[kind];
          node.setAttribute('aria-label', `${owned ? 'Your ' : ''}${buildingNames[kind]}`);
        }
        if (node.dataset.native !== String(native)) node.dataset.native = String(native);
        live.add(id);
        hits.push({ x: left + w / 2, y: top + h / 2, w, h, world: placed[index].world });
      }
      for (const [id,node] of nodes) if (!live.has(id)) { node.remove(); nodes.delete(id); }
      return { fields, borders: ownership ? borders : 0, icons: placed.length, sprites };
    },
    place(index) { return placed[index]; },
    // Counts for garrisons and the work around each place, drawn once units are binned.
    chips(context, scale) { drawChips(context, placed, scale); },
    pick(x,y) { return hits.find(p => Math.abs(x-p.x)<=p.w/2 && Math.abs(y-p.y)<=p.h/2)?.world ?? null; },
    reset() { rows = []; hits = []; ranked = []; placed = []; signature = ''; clear(); layer.replaceChildren(); nodes.clear(); },
  };
}
