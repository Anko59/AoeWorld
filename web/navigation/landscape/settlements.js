import { buildingKinds, buildingNames } from '/interface/menu/catalog.js';
import { actorColour } from '/minimap.js';
import { settlementSymbols } from '/navigation/landscape/symbols.js';
import { buildingSprite, loadSprites } from '/navigation/landscape/sprites.js';
import { drawChips } from '/navigation/landscape/chips.js';

// Minimum on-screen width (CSS px) per building kind; true size takes over when zoomed in.
const MIN = { 0: 84, 16: 88, 17: 56, 18: 56, 19: 56, 1: 30, 2: 34, 3: 34, 4: 40, 8: 28, 14: 30 };
const RANK = { 0: 9, 16: 8, 17: 6, 18: 5, 19: 5, 6: 4, 13: 4, 10: 4, 11: 4, 9: 4, 12: 3, 8: 3, 14: 3, 4: 2, 2: 2, 3: 2, 1: 1 };

// Batch footprints in world tiles so a regional redraw only submits the
// geometry that can intersect the camera. The symbol layer folds overlaps.
const STRIDE = 10, GROUP_TILES = 128;
const rankOf = row => (RANK[row[5]] ?? 1) * 2 + row[7];

function groupSites(values) {
  const groups = new Map();
  for (let at = 0; at + STRIDE <= values.length; at += STRIDE) {
    const row = Array.from(values.subarray(at, at + STRIDE));
    const [, x0, y0, x1, y1, kind, colour, owned] = row;
    if (!row.every(Number.isFinite) || (owned !== 0 && owned !== 1)
      || !buildingKinds[kind] || x1 <= x0 || y1 <= y0) continue;
    const key = `${Math.floor(x0 / GROUP_TILES)},${Math.floor(y0 / GROUP_TILES)}`;
    let group = groups.get(key);
    if (!group) {
      group = { bounds: [x0, y0, x1, y1], plots: new Map(), farms: null, furrows: null,
        fields: 0, borders: 0, symbols: [] };
      groups.set(key, group);
    }
    const box = group.bounds;
    box[0] = Math.min(box[0], x0); box[1] = Math.min(box[1], y0);
    box[2] = Math.max(box[2], x1); box[3] = Math.max(box[3], y1);
    if (kind === 5) {
      group.fields++;
      group.farms ??= new Path2D();
      group.furrows ??= new Path2D();
      group.farms.rect(x0, y0, x1 - x0, y1 - y0);
      for (let n = 1; n < 4; n++) {
        const x = x0 + (x1 - x0) * n / 4;
        group.furrows.moveTo(x, y0); group.furrows.lineTo(x, y1);
      }
    } else {
      group.borders++;
      let path = group.plots.get(colour);
      if (!path) { path = new Path2D(); group.plots.set(colour, path); }
      path.rect(x0, y0, x1 - x0, y1 - y0);
    }
    if (![5, 7, 15, 20, 21, 22, 23].includes(kind)) {
      const rank = rankOf(row);
      (group.symbols[rank] ??= []).push(row);
    }
  }
  return [...groups.values()];
}

function* visibleSymbols(groups) {
  for (let rank = RANK[0] * 2 + 1; rank >= 0; rank--) {
    for (const group of groups) {
      if (group.symbols[rank]) yield* group.symbols[rank];
    }
  }
}

export function initSettlements() {
  const layer = document.createElement('div');
  layer.id = 'strategic-settlements'; layer.hidden = true;
  document.body.append(layer);
  const nodes = new Map(), symbolFor = settlementSymbols();
  let placed = [], hits = [], snapshot = new Float64Array(), groups = [];
  return {
    update(values) {
      if (values.length === snapshot.length) {
        let at = 0;
        while (at < values.length && snapshot[at] === values[at]) at++;
        if (at === values.length) return false;
      }
      snapshot = Float64Array.from(values);
      groups = groupSites(snapshot);
      return true;
    },
    show(value) { layer.hidden = !value; },
    draw(context, project, ownership, width, height, scale, plane, zoom, space) {
      const visible = [];
      let fields = 0, borders = 0;
      const margin = Math.max(176 / scale, 256 * zoom);
      plane(); context.lineWidth = 1 / (64 * zoom);
      for (const group of groups) {
        const [x0, y0, x1, y1] = group.bounds;
        const [left] = project([x0, y1]), [right] = project([x1, y0]);
        const [, top] = project([x0, y0]), [, bottom] = project([x1, y1]);
        if (left <= width && right >= 0 && top <= height && bottom >= 0) {
          if (group.farms) {
            context.fillStyle = '#baa05a'; context.fill(group.farms);
            context.strokeStyle = '#6c5936'; context.stroke(group.farms);
            // Below a pixel per tile the field outline already shows the plot.
            if (64 * zoom >= 1) context.stroke(group.furrows);
            fields += group.fields;
          }
          if (ownership) for (const [colour, path] of group.plots) {
            context.fillStyle = context.strokeStyle = actorColour(colour);
            context.globalAlpha = .22; context.fill(path); context.globalAlpha = 1; context.stroke(path);
          }
          if (ownership) borders += group.borders;
        }
        // Sprites can extend beyond their footprint, including just outside the view.
        if (left <= width + margin && right >= -margin && top <= height + margin && bottom >= -margin) {
          visible.push(group);
        }
      }
      context.setTransform(1, 0, 0, 1, 0, 0);
      loadSprites();
      // One strong symbol per place: highest rank first, each symbol reserves
      // its screen rectangle, and anything that would overlap folds into it as a count.
      placed = []; hits = [];
      const live = new Set();
      let sprites = 0;
      for (const row of visibleSymbols(visible)) {
        const [id,x0,y0,x1,y1,kind,colour,owned,native,owner] = row;
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
        const at = space.owner(...r);
        if (at !== undefined) {
          if (at >= 0) placed[at]?.fold(kind);
          continue;
        }
        const index = placed.length;
        space.take(...r, index);
        placed.push({ id, x, y, w, h, left, top, kind, colour, owned, owner, buildings: 1, units: new Map(), world: [(x0+x1)/2,(y0+y1)/2],
          fold() { this.buildings++; } });
        const tint = actorColour(colour), base = w * .42;
        context.fillStyle = '#0006'; context.beginPath(); context.ellipse(x, y + w * .04, base * 1.05, base * .52, 0, 0, Math.PI * 2); context.fill();
        context.beginPath(); context.ellipse(x, y, base, base * .5, 0, 0, Math.PI * 2);
        context.globalAlpha = owned ? .62 : .38; context.fillStyle = tint; context.fill(); context.globalAlpha = 1;
        context.lineWidth = owned ? 2 / scale : 1 / scale; context.strokeStyle = owned ? '#fff8d6' : '#2b2116'; context.stroke();
        if (sprite) { context.drawImage(sprite.canvas, left, top, w, h); sprites++; }
        else { const icon = symbolFor(kind); if (icon) context.drawImage(icon, x - w / 2, y - w * .75, w, w); }
        // Accessible labels are retained for every drawn place.
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
      return { fields, borders, icons: placed.length, sprites };
    },
    place(index) { return placed[index]; },
    // Counts for garrisons and the work around each place, drawn once units are binned.
    chips(context, scale, artFor) { drawChips(context, placed, scale, artFor); },
    pick(x,y) { return hits.find(p => Math.abs(x-p.x)<=p.w/2 && Math.abs(y-p.y)<=p.h/2)?.world ?? null; },
    reset() { groups = []; hits = []; placed = []; snapshot = new Float64Array(); layer.replaceChildren(); nodes.clear(); },
  };
}
