import { entityIcon } from '/interface/ui-art.js';
const states = ['idle', 'marching', 'fighting', 'retreating'];
const stances = ['Aggressive', 'Defensive', 'Stand ground', 'No attack'];
const roles = ['Infantry', 'Archers', 'Cavalry', 'Siege', 'Scouts'];
const waits = ['Training villagers', 'Census busy', 'At capacity', 'No Town Center', 'Food shortage', 'Housing shortage'];
const teamColours = { blue:'#4191f5', red:'#f54141', green:'#32be41', yellow:'#f5dc28', cyan:'#28dcdc', purple:'#af41dc', grey:'#a0a0a0', orange:'#f59128' };

// Retained nodes, bounded independently of world size. Coordinates and hit
// boxes share the exact plane used for ordinary ground orders.
export function initTokens() {
  const layer = document.createElement('div');
  layer.id = 'strategic-tokens'; layer.hidden = true;
  document.body.append(layer);
  const nodes = new Map();
  let armies = [], economy = [], bridges = [], hits = [], domainSignature = '';
  let hudRects = null, hudRectsAt = 0, hudRectsFor = '';
  const domains = new Map();
  function button(key, className) {
    let node = nodes.get(key);
    if (node) return node;
    node = document.createElement('button');
    node.type = 'button'; node.className = className;
    // Forward native pointer input to the ordinary Rust selection/order path.
    // The layer owns presentation; it has no independent gameplay callback.
    const forward = event => {
      event.preventDefault();
      const scene = document.getElementById('scene');
      const bounds = node.getBoundingClientRect();
      const x = bounds.left + bounds.width / 2, y = bounds.top + bounds.height / 2;
      scene.dispatchEvent(new PointerEvent('pointerdown', {
        clientX: x, clientY: y, button: event.button ?? 0,
        ctrlKey: event.ctrlKey, altKey: event.altKey, shiftKey: event.shiftKey,
      }));
    };
    node.addEventListener('pointerdown', forward);
    node.addEventListener('click', event => { if (event.detail === 0) forward(event); });
    node.addEventListener('contextmenu', event => event.preventDefault());
    nodes.set(key, node); layer.append(node);
    return node;
  }
  function rows(values, stride, cap) {
    const result = [];
    for (let at = 0; at + stride <= values.length && result.length < cap; at += stride) {
      const row = Array.from(values.slice(at, at + stride));
      if (row.slice(0, 4).every(Number.isFinite)) result.push(row);
    }
    return result;
  }
  function place(node, x, y, visible, width, height, id, scale) {
    const sx = x * scale, sy = y * scale;
    node.hidden = !visible;
    if (!visible) return;
    if (node.dataset.at !== `${sx},${sy}`) { node.dataset.at = `${sx},${sy}`; node.style.left = `${sx}px`; node.style.top = `${sy}px`; }
    hits.push({ id, x, y, width: width / scale, height: height / scale });
  }
  return {
    invalidate() { hudRects = null; },
    show(value) {
      if (layer.hidden === !value) return;
      layer.hidden = !value; if (!value) hits = [];
    },
    update(a, e, b) {
      armies = rows(a, 15, 256); economy = rows(e, 13, 64); bridges = rows(b, 5, 2048);
      const next = economy.map(r => [r[0], r[2], r[3], r[7] === -1]).flat().join(',');
      if (next !== domainSignature) {
        domainSignature = next; domains.clear();
        for (const [id, , wx, wy, , , , wait] of economy) {
          const ring = [];
          for (let k = 0; k <= 64; k++) {
            const angle = k / 64 * Math.PI * 2, dx = Math.cos(angle), dy = Math.sin(angle);
            let reach = 100;
            if (wait !== -1) for (const other of economy) {
              if (other[0] === id || other[7] === -1) continue;
              const ox = other[2] - wx, oy = other[3] - wy, distance = Math.hypot(ox, oy), along = ox * dx + oy * dy;
              if (distance > 0 && distance < 200 && along > 1e-9) reach = Math.min(reach, Math.max(0, distance / 2 - .4) * distance / along);
            }
            ring.push([wx + reach * dx, wy + reach * dy]);
          }
          domains.set(id, ring);
        }
      }
      const live = new Set([...armies.map(r => `a${r[0]}`), ...economy.map(r => `e${r[0]}`)]);
      for (const [key, node] of nodes) if (!live.has(key)) { node.remove(); nodes.delete(key); }
    },
    entity(x, y) {
      if (layer.hidden) return NaN;
      // Last drawn nodes are on top, matching DOM paint order.
      return [...hits].reverse().find(h => Math.abs(x - h.x) <= h.width / 2
        && Math.abs(y - h.y) <= h.height / 2)?.id ?? NaN;
    },
    // HUD rectangles are reserved first; reading them forces layout, so they are cached for 250 ms (HUD cadence).
    hud(space, width, height) {
      const stamp = performance.now();
      if (!hudRects || stamp - hudRectsAt > 250 || hudRectsFor !== `${width}x${height}`) {
        hudRects = [...document.querySelectorAll('#strategic-cockpit,#cockpit-details,#far-dock,#general-panel,#bottom-panel,#strategic-map-key,#strategic-label,#performance-overlay')]
          .map(panel => panel.getBoundingClientRect());
        hudRectsAt = stamp; hudRectsFor = `${width}x${height}`;
      }
      for (const r of hudRects) if (r.width && r.height) space.take(r.left - 4, r.top - 4, r.right + 4, r.bottom + 4, -1);
    },
    mine() { const c = teamColours[document.getElementById('playground')?.dataset.playerColour] ?? teamColours.blue; return parseInt(c.slice(1), 16); },
    draw(context, project, options, scale, width, height, space) {
      hits = [];
      const holdingColour = teamColours[document.getElementById('playground')?.dataset.playerColour] ?? teamColours.blue;
      const inView = (x, y) => x >= -100 && y >= -60 && x <= width + 100 && y <= height + 60;
      const positions = new Map();
      let grouped = 0;
      // A small screen grid keeps nearby armies and cards individually usable.
      // Selected armies get their anchor first; displaced labels keep a leader
      // line to the actual world position. This never scans gameplay units.
      // Crowded kingdoms shrink Steward plaques to icons so they never hide land.
      const dense = economy.length > 12;
      const density = dense ? 'high' : 'normal';
      if (layer.dataset.density !== density) layer.dataset.density = density;
      const labels = [
        ...armies.filter(() => options.armies).map(r => ({ key: `a${r[0]}`, world: r.slice(1, 3), selected: r[4], owned: r[3], w: 84, h: 38 })),
        ...economy.filter(() => options.economy && !dense).map(r => ({ key: `e${r[0]}`, world: r.slice(2, 4), selected: r[4], owned: 1, w: 118, h: 34 })),
      ].sort((a, b) => b.selected - a.selected || b.owned - a.owned);
      for (const label of labels) {
        const [ax, ay] = project(label.world).map(v => v * scale);
        if (!inView(ax / scale, ay / scale)) continue;
        for (let attempt = 0; attempt < 65; attempt++) {
          const ring = Math.ceil(attempt / 8), angle = (attempt - 1) % 8 * Math.PI / 4;
          const x = Math.max(label.w / 2 + 8, Math.min(width * scale - label.w / 2 - 8, ax + (attempt ? Math.cos(angle) * ring * 92 : 0)));
          const y = Math.max(120 + label.h / 2, Math.min(height * scale - 190 - label.h / 2, ay + (attempt ? Math.sin(angle) * ring * 56 : 0)));
          const box = [x - label.w / 2 - 3, y - label.h / 2 - 3, x + label.w / 2 + 3, y + label.h / 2 + 3];
          if (!space.free(...box)) continue;
          space.take(...box, -3); positions.set(label.key, [x / scale, y / scale]);
          if (Math.hypot(x - ax, y - ay) > 4) {
            context.strokeStyle = '#f4e9bb99'; context.lineWidth = 1;
            context.beginPath(); context.moveTo(ax / scale, ay / scale); context.lineTo(x / scale, y / scale); context.stroke();
          }
          break;
        }
        if (!positions.has(label.key)) grouped++;
      }
      if (dense && options.economy) for (const r of economy) {
        // One flag per town, tied to its Town Center sprite instead of floating free.
        const [ax, ay] = project(r.slice(2, 4)).map(v => v * scale);
        positions.set(`e${r[0]}`, [(ax + 38) / scale, (ay - 40) / scale]);
        space.take(ax + 20, ay - 56, ax + 56, ay - 24, -3);
      }
      if (layer.dataset.grouped !== String(grouped)) layer.dataset.grouped = String(grouped);
      if (options.roads) {
        context.strokeStyle = '#f8dfac'; context.lineWidth = 4;
        for (const [x0, y0, x1, y1, kind] of bridges) {
          if (kind === 23) {
            const corners = [[x0,y0],[x1,y0],[x1,y1],[x0,y1]].map(project);
            context.fillStyle = '#d6b57b'; context.beginPath(); context.moveTo(...corners[0]);
            for (const p of corners.slice(1)) context.lineTo(...p);
            context.closePath(); context.fill(); continue;
          }
          const a = project([x0, y0]), b = project([x1, y1]);
          context.beginPath(); context.moveTo(...a); context.lineTo(...b); context.stroke();
        }
      }
      for (const r of economy) {
        const [id, steward, wx, wy, selected, managed, residents, wait, f, w, g, s, idle] = r;
        const node = button(`e${id}`, 'steward-card');
        const [x, y] = project([wx, wy]);
        const visible = options.economy && inView(x, y) && positions.has(`e${id}`);
        if (options.economy || options.ownership) {
          context.beginPath();
          const ring = domains.get(id) ?? [];
          for (let k = 0; k < ring.length; k++) {
            const p = project(ring[k]);
            if (k) context.lineTo(...p); else context.moveTo(...p);
          }
          if (options.ownership && wait !== -1) {
            context.fillStyle = holdingColour; context.globalAlpha = .13; context.fill(); context.globalAlpha = 1;
          }
          context.strokeStyle = wait === -1 ? '#d2c393' : holdingColour; context.lineWidth = selected ? 2.5 : 1.5;
          context.setLineDash(wait === -1 ? [5, 5] : []); context.stroke(); context.setLineDash([]);
        }
        const label = waits[wait] ?? (wait === -1 ? 'Inactive · garrison in Town Center' : 'Evaluating');
        const total = f + w + g + s + idle || 1;
        const signature = [managed, residents, f, w, g, s, idle, wait].join(',');
        if (node.dataset.signature !== signature) {
          node.dataset.signature = signature;
          const part = (n, colour) => `<i style="flex:${n};background:${colour}"></i>`;
          node.innerHTML = `<span class="sc-art">${entityIcon('Steward')}</span><span class="sc-body"><b>${managed < 0 ? '?' : managed}</b><small>${idle} idle</small></span>`
            + `<span class="sc-jobs">${part(f, '#d65c6a')}${part(w, '#c28a4b')}${part(g, '#f0c63e')}${part(s, '#aeb4c0')}${part(idle, '#555')}</span>`;
          node.dataset.state = wait === -1 ? 'inactive' : wait >= 0 && wait !== 0 ? 'waiting' : 'working';
          node.title = `Steward #${steward} · ${managed < 0 ? '?' : managed} managed / ${residents < 0 ? '?' : residents} residents\nObserved jobs: food ${f} · wood ${w} · gold ${g} · stone ${s} · ${idle} idle\n${label}\nClick to select the Town Center and open Steward controls`;
        }
        node.dataset.castle = String(id); node.dataset.steward = String(steward);
        node.setAttribute('aria-pressed', String(!!selected));
        const position = positions.get(`e${id}`) ?? [x, y];
        place(node, ...position, visible, dense ? 40 : 118, dense ? 38 : 34, -id - 1, scale);
      }
      for (const r of armies) {
        const [id, wx, wy, owned, selected, size, state, stance, tx, ty, ...counts] = r;
        const node = button(`a${id}`, 'army-token');
        const [x, y] = project([wx, wy]);
        const order = owned ? states[state] ?? 'idle' : 'visible enemy';
        const signature = [size, order, stance, ...counts].join(',');
        if (node.dataset.signature !== signature) {
          const total = counts.reduce((a, b) => a + b, 0);
          node.replaceChildren();
          const glyph = { marching: '➜', fighting: '⚔', retreating: '↩' }[order] ?? '';
          const title = document.createElement('b'); title.textContent = `${size < 0 ? '?' : size}${glyph ? ' ' + glyph : ''}`;
          const mode = document.createElement('small'); mode.textContent = owned ? (stances[stance] ?? '') : 'enemy';
          const bar = document.createElement('span'); bar.className = 'army-mix';
          counts.forEach((count, i) => {
            const part = document.createElement('i');
            part.style.width = `${total ? count / total * 100 : 0}%`;
            part.style.background = ['#d7c19c', '#7ed19a', '#85bdf0', '#e6a177', '#dedc7a'][i];
            bar.append(part);
          });
          node.append(title, mode, bar); node.dataset.signature = signature;
          node.title = `General #${id} · ${order} · ${stances[stance] ?? 'unknown stance'}\n${size < 0 ? 'Enemy army details unknown' : roles.map((name, i) => `${name}: ${counts[i]}`).join(' · ')}\nClick to select · right-click ground to march · Alt: General alone`;
        }
        node.dataset.entity = String(id); node.dataset.owned = String(!!owned); node.dataset.order = order;
        node.setAttribute('aria-pressed', String(!!selected));
        const visible = options.armies && inView(x, y) && positions.has(`a${id}`);
        if (visible && owned && Number.isFinite(tx) && Number.isFinite(ty)) {
          const [ex, ey] = project([tx, ty]);
          const dx = ex - x, dy = ey - y, distance = Math.hypot(dx, dy);
          if (distance > 1) {
            const length = Math.min(distance, 140 / scale), ux = dx / distance, uy = dy / distance;
            const ax = x + ux * length, ay = y + uy * length;
            context.strokeStyle = '#fff2b0'; context.lineWidth = selected ? 3 : 2;
            context.beginPath(); context.moveTo(x, y); context.lineTo(ax, ay);
            context.moveTo(ax - ux * 9 - uy * 5, ay - uy * 9 + ux * 5);
            context.lineTo(ax, ay); context.lineTo(ax - ux * 9 + uy * 5, ay - uy * 9 - ux * 5); context.stroke();
          }
        }
        const position = positions.get(`a${id}`) ?? [x, y];
        place(node, ...position, visible, 84, 38, id, scale);
      }
    },
  };
}
