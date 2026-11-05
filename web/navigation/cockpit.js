import { icon, entityIcon, trainingKinds, panelArt } from '/interface/ui-art.js';
import { initFarBuilding } from '/navigation/building.js';
import { initProduction } from '/navigation/production.js';

// One retained panel; values are reports, never estimates from stock changes.
export function initCockpit(changed = () => {}) {
  const panel = document.createElement('aside');
  panel.id = 'strategic-cockpit'; panel.hidden = true;
  panel.setAttribute('aria-label', 'Kingdom cockpit');
  panel.innerHTML = `<button type="button" aria-label="Kingdom controls" aria-expanded="true" aria-controls="cockpit-details">Kingdom controls ▾</button>
    <div class="cockpit-resources">${['wood', 'food', 'gold', 'stone'].map(name =>
      `<div>${icon('resources', name, name)}<span><b data-stock="${name}">?</b><small data-income="${name}">?</small></span></div>`).join('')}</div>
    <span class="cockpit-population">Population <b data-population>?</b></span><b data-army-total>Army ?</b>
    <section id="cockpit-details" aria-label="Kingdom controls">
    <header><b>Kingdom cockpit</b><span>Held · collected / minute</span></header>
    <div class="cockpit-census"><span>Land held <b data-land>?</b></span></div>
    <p class="cockpit-note">Collections: last minute in 5s buckets (55–60s).</p>
    <p class="cockpit-note">Buildings and active Steward / Vassal domains. Borders shown only on explored land.</p><p class="cockpit-note" data-land-coverage></p>
    <div class="cockpit-army" aria-label="Army by type"></div></section>`;
  document.body.append(panel);
  const production = initProduction();
  const details = panel.querySelector('#cockpit-details');
  const building = initFarBuilding(details);
  const toggle = panel.querySelector('[aria-controls]');
  let collapsed = false, identity = '', tokenSeen = null, generation = 0;
  function collapse(value) {
    collapsed = value; details.hidden = value;
    toggle.setAttribute('aria-expanded', String(!value));
    toggle.textContent = `Kingdom controls ${value ? '▸' : '▾'}`;
  }
  function persist() {
    if (!identity) return;
    try { sessionStorage.setItem('aoeworld.cockpit-layout', JSON.stringify({ identity, collapsed })); } catch { /* In-memory choice remains usable. */ }
  }
  toggle.addEventListener('click', () => { generation++; collapse(!collapsed); persist(); changed(); });
  function fnv(text) {
    let a = 0x811c9dc5, b = 0x01000193;
    for (let i = 0; i < text.length; i++) { a = Math.imul(a ^ text.charCodeAt(i), 0x01000193) >>> 0; b = Math.imul(b ^ a, 0x85ebca6b) >>> 0; }
    return a.toString(16).padStart(8, '0') + b.toString(16).padStart(8, '0');
  }
  async function restore() {
    let token = '';
    try { token = localStorage.getItem('aoeworld.resume-token') ?? ''; } catch { /* In-memory fallback. */ }
    if (token === tokenSeen) return;
    tokenSeen = token;
    const current = ++generation;
    identity = '';
    if (!token) return;
    let key;
    try {
      // crypto.subtle exists only on secure origins; others use a local fallback key.
      key = Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(token))),
        b => b.toString(16).padStart(2, '0')).join('');
    } catch { key = 'fnv:' + fnv(token); }
    if (token !== tokenSeen) return;
    identity = key;
    if (current !== generation) { persist(); return; }
    let saved;
    try { saved = JSON.parse(sessionStorage.getItem('aoeworld.cockpit-layout') ?? 'null'); } catch { /* Default expanded. */ }
    const next = saved?.identity === identity && saved.collapsed === true;
    if (next !== collapsed) { collapse(next); changed(); }
  }
  let signature = '';
  const number = value => Number.isFinite(value) ? value.toLocaleString('en-US') : '?';
  function text(selector, value) {
    const node = panel.querySelector(selector);
    if (node.textContent !== value) node.textContent = value;
  }
  function update(values = [], census = []) {
    production.ready(Number.isFinite(values[4]) && document.getElementById('connection')?.textContent === 'connected');
    panelArt(panel, 'selection'); panelArt(details, 'selection');
    // Decode borrowed WASM slices immediately; no retained WASM-memory view.
    ['wood', 'food', 'gold', 'stone'].forEach((name, index) => {
      text(`[data-stock="${name}"]`, number(values[index]));
      text(`[data-income="${name}"]`, `${number(values[6 + index])} / min`);
    });
    text('[data-population]', `${number(values[4])} / ${number(values[5])}`);
    const share = !Number.isFinite(values[11]) ? '?' : values[11] === 0 && values[10] > 0
      ? '<0.0001' : (values[11] / 10000).toFixed(values[11] > 0 && values[11] < 100 ? 4 : 2);
    text('[data-land]', `${number(values[10])} tiles · ${share}% of map`);
    text('[data-land-coverage]', values[12] === 0 ? "Boundary detail limited; area total is complete." : "");
    const counts = new Map();
    for (let kind = 0; kind < Math.min(census.length, trainingKinds.length); kind++) {
      const count = census[kind];
      if (!Number.isInteger(count) || count <= 0 || [0, 3, 22, 34].includes(kind)) continue;
      counts.set(kind, count);
    }
    const rows = [...counts].sort((a, b) => a[0] - b[0]);
    const known = census.length === trainingKinds.length;
    text('[data-army-total]', `Army ${known ? number(rows.reduce((sum, [, n]) => sum + n, 0)) : '?'}`);
    const next = JSON.stringify([known, rows]);
    if (next !== signature) {
      signature = next;
      const army = panel.querySelector('.cockpit-army');
      army.innerHTML = `<b>Army · ${known ? rows.reduce((sum, [, n]) => sum + n, 0) : '?'}</b>`
        + rows.map(([kind, count]) => `<span>${entityIcon(trainingKinds[kind])}${trainingKinds[kind]}: ${number(count)}</span>`).join('');
    }
  }
  return { update, production, building,
    mount() {
      for (const id of ['strategic-layers', 'strategic-views', 'units-here']) {
        const node = document.getElementById(id);
        if (node) details.append(node);
      }
    },
    show(value) { if (value) void restore(); panel.hidden = !value; production.show(value); building.show(value); }, reset() { generation++; tokenSeen = null; identity = ''; collapse(false); signature = ''; production.reset(); building.reset(); update(); } };
}
