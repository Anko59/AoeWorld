import { icon, entityIcon, trainingKinds, techIds } from '/interface/ui-art.js';
import { initFarBuilding } from '/navigation/building.js';
import { initProduction } from '/navigation/production.js';
import { applyFarArt } from '/navigation/cockpit/art.js';
import { initResearch } from '/navigation/cockpit/research.js';
import { initTrade } from '/navigation/cockpit/trade.js';
import { createHistory, drawChart, resourceColours, armyClasses, armyClassOf } from '/navigation/cockpit/history.js';

const tabs = [
  ['build', 'Build', () => icon('commands', 'build', 'towncenter'), 'Build · place any building from here'],
  ['train', 'Train', () => entityIcon('Militia'), 'Train units and send them to a destination'],
  ['research', 'Research', () => icon('technologies', techIds[22] ?? 22, 'technology'), 'Research technologies at your buildings'],
  ['trade', 'Trade', () => entityIcon('Market'), 'Market · buy and sell wood, food and stone'],
  ['stats', 'Stats', () => icon('resources', 'population', 'villager'), 'Resource and army history'],
];
const titles = { build: 'Build', train: 'Train units', research: 'Research', trade: 'Market', stats: 'Kingdom history' };

// One retained dock: an icon rail and one drawer. Values are reports, never
// estimates from stock changes.
export function initCockpit(changed = () => {}) {
  applyFarArt();
  const link = document.createElement('link');
  link.rel = 'stylesheet'; link.href = '/navigation/cockpit/panel.css'; document.head.append(link);
  const panel = document.createElement('aside');
  panel.id = 'strategic-cockpit'; panel.hidden = true;
  panel.setAttribute('aria-label', 'Kingdom cockpit');
  panel.innerHTML = `<nav class="far-rail far-stone" role="tablist" aria-label="Kingdom controls">${tabs.map(([id, name, art, hint]) =>
    `<button type="button" class="far-tab" role="tab" data-tab="${id}" aria-selected="false" aria-controls="cockpit-details" title="${hint}">${art()}<span>${name}</span></button>`).join('')}</nav>
    <section id="cockpit-details" class="far-drawer far-frame" aria-label="Kingdom controls">
      <header><b data-title></b><button type="button" data-collapse aria-label="Collapse kingdom controls" aria-expanded="true">‹</button></header>
      ${tabs.map(([id]) => `<div class="far-body" data-body="${id}" hidden></div>`).join('')}
    </section>`;
  document.body.append(panel);
  const details = panel.querySelector('#cockpit-details');
  const body = id => panel.querySelector(`[data-body="${id}"]`);
  const production = initProduction();
  body('train').append(production.form);
  const building = initFarBuilding(body('build'));
  const research = initResearch(body('research'));
  const trade = initTrade(body('trade'));
  const history = createHistory();
  body('stats').innerHTML = `<div class="far-group">Stockpile · each line scaled to its own peak</div><div class="far-legend">${['wood', 'food', 'gold', 'stone'].map(n => `<span><i style="background:${resourceColours[n]}"></i>${n}</span>`).join('')}</div>
    <canvas class="far-chart" data-chart="stock" aria-label="Stockpile over time"></canvas>
    <div class="far-group">Collected per minute</div>
    <canvas class="far-chart" data-chart="rate" aria-label="Resources collected per minute over time"></canvas>
    <div class="far-group">Army by type</div><div class="far-legend">${armyClasses.map(([n, c]) => `<span><i style="background:${c}"></i>${n}</span>`).join('')}</div>
    <canvas class="far-chart" data-chart="army" aria-label="Army size over time"></canvas>
    <div class="far-group">Population</div>
    <canvas class="far-chart" data-chart="pop" aria-label="Population and cap over time"></canvas>
    <div class="cockpit-army far-legend" aria-label="Army by type"></div>
    <p class="far-hint">History is sampled every 5 seconds from the same reports as the top bar; nothing is extrapolated. Land held <b data-land>?</b>. <span data-land-coverage></span></p>`;
  let collapsed = true, active = 'build', identity = '', touched = false, tokenSeen = null, generation = 0, chartRevision = -1, latest = [];
  const toggle = panel.querySelector('[data-collapse]');
  function select(id, open = true) {
    active = id;
    for (const [tab] of tabs) {
      panel.querySelector(`[data-tab="${tab}"]`).setAttribute('aria-selected', String(tab === id && open));
      body(tab).hidden = tab !== id;
    }
    panel.querySelector('[data-title]').textContent = titles[id];
    collapse(!open);
    if (open && id === 'stats') drawCharts(true);
  }
  function collapse(value) {
    collapsed = value; details.hidden = value;
    toggle.setAttribute('aria-expanded', String(!value));
    panel.dataset.collapsed = String(value);
  }
  const persist = () => {
    if (!identity) return;
    try { sessionStorage.setItem('aoeworld.cockpit-layout', JSON.stringify({ identity, collapsed, tab: active })); } catch { /* In-memory choice remains usable. */ }
  };
  panel.querySelectorAll('[data-tab]').forEach(tab => tab.addEventListener('click', () => {
    generation++; touched = !identity;
    select(tab.dataset.tab, collapsed || active !== tab.dataset.tab);
    persist(); changed();
  }));
  toggle.addEventListener('click', () => { generation++; touched = !identity; select(active, false); persist(); changed(); });
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
    // A choice made before the session token existed is the user's latest and must win over an older save.
    if (current !== generation || touched) { touched = false; persist(); return; }
    let saved;
    try { saved = JSON.parse(sessionStorage.getItem('aoeworld.cockpit-layout') ?? 'null'); } catch { /* Default layout. */ }
    if (saved?.identity === identity) {
      const next = tabs.some(([id]) => id === saved.tab) ? saved.tab : 'build';
      if (next !== active || saved.collapsed !== collapsed) { select(next, saved.collapsed !== true); changed(); }
    }
  }
  let signature = '';
  const number = value => Number.isFinite(value) ? value.toLocaleString('en-US') : '?';
  function text(selector, value) {
    const node = panel.querySelector(selector);
    if (node && node.textContent !== value) node.textContent = value;
  }
  let drawn = 0;
  function drawCharts(force) {
    if (collapsed || active !== 'stats') return;
    const now = performance.now();
    if (!force && (history.revision() === chartRevision || now - drawn < 900)) return;
    chartRevision = history.revision(); drawn = now;
    const chart = name => panel.querySelector(`[data-chart="${name}"]`);
    const names = ['wood', 'food', 'gold', 'stone'], colours = names.map(n => resourceColours[n]);
    drawChart(chart('stock'), history, names.map(n => `stock-${n}`), colours, { normalize: true });
    drawChart(chart('rate'), history, names.map(n => `rate-${n}`), colours);
    drawChart(chart('army'), history, armyClasses.map((_, i) => `class-${i}`), armyClasses.map(c => c[1]), { stacked: true });
    drawChart(chart('pop'), history, ['pop', 'cap'], ['#7cc4f0', '#d9c39b']);
  }
  function update(values = [], census = [], now = performance.now()) {
    production.ready(Number.isFinite(values[4]) && document.getElementById('connection')?.textContent === 'connected');
    latest = values;
    const share = !Number.isFinite(values[11]) ? '?' : values[11] === 0 && values[10] > 0
      ? '<0.0001' : (values[11] / 10000).toFixed(values[11] > 0 && values[11] < 100 ? 4 : 2);
    text('[data-land]', `${number(values[10])} tiles · ${share}% of map`);
    text('[data-land-coverage]', values[12] === 0 ? 'Boundary detail limited; area total is complete.' : '');
    const counts = new Map(), classes = armyClasses.map(() => 0);
    for (let kind = 0; kind < Math.min(census.length, trainingKinds.length); kind++) {
      const count = census[kind];
      if (!Number.isInteger(count) || count <= 0 || [0, 3, 22, 34].includes(kind)) continue;
      counts.set(kind, count); classes[armyClassOf(trainingKinds[kind])] += count;
    }
    const rows = [...counts].sort((a, b) => a[0] - b[0]);
    const known = census.length === trainingKinds.length;
    if (known) history.sample(values, classes, now);
    const next = JSON.stringify([known, rows]);
    if (next !== signature) {
      signature = next;
      panel.querySelector('.cockpit-army').innerHTML = `<b>Army · ${known ? number(rows.reduce((sum, [, n]) => sum + n, 0)) : '?'}</b>`
        + rows.map(([kind, count]) => `<span>${entityIcon(trainingKinds[kind])} ${trainingKinds[kind]}: ${number(count)}</span>`).join('');
    }
    drawCharts(false);
  }
  select('build', false);
  return { update, production, building,
    commerce(report) { research.update(report); trade.update(report, latest); },
    mount() {
      const dock = document.createElement('div');
      dock.id = 'far-dock'; dock.hidden = true;
      for (const id of ['strategic-layers', 'strategic-views']) {
        const node = document.getElementById(id);
        if (!node) continue;
        const row = document.createElement('div'); row.className = 'dock-row far-stone';
        row.append(node); dock.append(row);
      }
      document.body.append(dock);
      this.dock = dock;
    },
    show(value) { if (value) void restore(); panel.hidden = !value; if (this.dock) this.dock.hidden = !value; production.show(value); building.show(value); },
    reset() { generation++; tokenSeen = null; identity = ''; select('build', false); signature = ''; history.reset(); chartRevision = -1; production.reset(); building.reset(); update(); } };
}
