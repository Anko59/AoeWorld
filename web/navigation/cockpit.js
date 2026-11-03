import { icon, entityIcon, trainingKinds, panelArt } from '/interface/ui-art.js';

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
  let signature = '';
  const number = value => Number.isFinite(value) ? value.toLocaleString('en-US') : '?';
  function text(selector, value) {
    const node = panel.querySelector(selector);
    if (node.textContent !== value) node.textContent = value;
  }
  function update(values = []) {
    panelArt(panel, 'selection');
    // Decode borrowed WASM slices immediately; no retained WASM-memory view.
    ['wood', 'food', 'gold', 'stone'].forEach((name, index) => {
      text(`[data-stock="${name}"]`, number(values[index]));
      text(`[data-income="${name}"]`, `${number(values[6 + index])} / min`);
    });
    text('[data-population]', `${number(values[4])} / ${number(values[5])}`);
    text('[data-land]', `${number(values[10])} tiles`);
    const counts = new Map();
    for (let at = 11; at + 1 < values.length; at += 2) {
      const kind = values[at], count = values[at + 1];
      if (!Number.isInteger(kind) || !trainingKinds[kind] || !Number.isInteger(count) || count <= 0
        || [0, 3, 22].includes(kind)) continue;
      counts.set(kind, (counts.get(kind) ?? 0) + count);
    }
    const rows = [...counts].sort((a, b) => a[0] - b[0]);
    const known = Number.isFinite(values[4]);
    const next = JSON.stringify([known, rows]);
    if (next !== signature) {
      signature = next;
      const army = panel.querySelector('.cockpit-army');
      army.innerHTML = `<b>Army · ${known ? rows.reduce((sum, [, n]) => sum + n, 0) : '?'}</b>`
        + rows.map(([kind, count]) => `<span>${entityIcon(trainingKinds[kind])}${trainingKinds[kind]}: ${number(count)}</span>`).join('');
    }
  }
  return { update, show(value) { panel.hidden = !value; }, reset() { signature = ''; update(); } };
}
