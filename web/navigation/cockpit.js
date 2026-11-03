import { icon, entityIcon, trainingKinds, panelArt } from '/interface/ui-art.js';
import { initProduction } from '/navigation/production.js';

// One retained panel; values are reports, never estimates from stock changes.
export function initCockpit() {
  const panel = document.createElement('aside');
  panel.id = 'strategic-cockpit'; panel.hidden = true;
  panel.setAttribute('aria-label', 'Kingdom cockpit');
  panel.innerHTML = `<header><b>Kingdom cockpit</b><span>Held · collected / minute</span></header>
    <div class="cockpit-resources">${['wood', 'food', 'gold', 'stone'].map(name =>
      `<div>${icon('resources', name, name)}<span><b data-stock="${name}">?</b><small data-income="${name}">?</small></span></div>`).join('')}</div>
    <div class="cockpit-census"><span>Population <b data-population>?</b></span><span>Occupied land <b data-land>?</b></span></div>
    <p class="cockpit-note">Collections: last minute in 5s buckets (55–60s). Land: building footprints, not borders.</p>
    <div class="cockpit-army" aria-label="Army by type"></div>`;
  document.body.append(panel);
  const production = initProduction();
  let signature = '';
  const number = value => Number.isFinite(value) ? value.toLocaleString('en-US') : '?';
  function text(selector, value) {
    const node = panel.querySelector(selector);
    if (node.textContent !== value) node.textContent = value;
  }
  function update(values = [], census = []) {
    production.ready(Number.isFinite(values[4]) && document.getElementById('connection')?.textContent === 'connected');
    panelArt(panel, 'selection');
    // Decode borrowed WASM slices immediately; no retained WASM-memory view.
    ['wood', 'food', 'gold', 'stone'].forEach((name, index) => {
      text(`[data-stock="${name}"]`, number(values[index]));
      text(`[data-income="${name}"]`, `${number(values[6 + index])} / min`);
    });
    text('[data-population]', `${number(values[4])} / ${number(values[5])}`);
    text('[data-land]', `${number(values[10])} tiles`);
    const counts = new Map();
    for (let kind = 0; kind < Math.min(census.length, trainingKinds.length); kind++) {
      const count = census[kind];
      if (!Number.isInteger(count) || count <= 0 || [0, 3, 22].includes(kind)) continue;
      counts.set(kind, count);
    }
    const rows = [...counts].sort((a, b) => a[0] - b[0]);
    const known = census.length === trainingKinds.length;
    const next = JSON.stringify([known, rows]);
    if (next !== signature) {
      signature = next;
      const army = panel.querySelector('.cockpit-army');
      army.innerHTML = `<b>Army · ${known ? rows.reduce((sum, [, n]) => sum + n, 0) : '?'}</b>`
        + rows.map(([kind, count]) => `<span>${entityIcon(trainingKinds[kind])}${trainingKinds[kind]}: ${number(count)}</span>`).join('');
    }
  }
  return { update, production, show(value) { panel.hidden = !value; production.show(value); }, reset() { signature = ''; production.reset(); update(); } };
}
