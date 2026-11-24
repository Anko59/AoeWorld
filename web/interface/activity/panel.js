import { entityIcon, icon, trainingKinds, techIds } from '/interface/ui-art.js';
import { set, attribute } from '/interface/dom/update.js';
import { names, techKinds, techName } from '/interface/menu/catalog.js';

export function initActivity() {
  const panel = document.createElement('section');
  panel.id = 'kingdom-activity'; panel.className = 'far-stone'; panel.hidden = true;
  panel.setAttribute('aria-label', 'Kingdom activity');
  panel.innerHTML = '<h2>Kingdom activity</h2><div class="activity-rows"></div>';
  document.getElementById('playground').append(panel);
  const host = panel.querySelector('.activity-rows');
  const rows = new Map();
  function update(values) {
    const live = new Set(), total = Math.floor(values.length / 5);
    for (let i = 0; i < total; i++) {
      const [research, kind, count, progress, building] = values.slice(i * 5, (i + 1) * 5);
      const key = `${research}:${kind}`;
      live.add(key);
      let row = rows.get(key);
      const name = research ? techName(techKinds[kind]) : names[trainingKinds[kind]];
      if (!row) {
        const button = document.createElement('button'); button.type = 'button';
        button.innerHTML = `${research ? icon('technologies', techIds[kind], 'technology') : entityIcon(trainingKinds[kind])}<span><b></b><progress max="1000"></progress></span><strong></strong>`;
        row = { button, name: button.querySelector('b'), count: button.querySelector('strong'), progress: button.querySelector('progress'), building };
        button.addEventListener('click', () => { window.aoeBuildings?.command(14, 0, row.building, 0); document.getElementById('scene').focus(); });
        rows.set(key, row);
      }
      row.building = building;
      set(row.name, 'textContent', name); set(row.count, 'textContent', `×${count}`);
      set(row.progress, 'value', progress);
      attribute(row.progress, 'value', String(progress));
      attribute(row.progress, 'aria-label', `${name} progress`);
      const label = `${name} ×${count} · ${Math.floor(progress / 10)}% · Go to producing building`;
      set(row.button, 'title', label); attribute(row.button, 'aria-label', label);
      if (host.children[i] !== row.button) host.insertBefore(row.button, host.children[i] ?? null);
    }
    for (const [key, row] of rows) if (!live.has(key)) { row.button.remove(); rows.delete(key); }
    panel.style.setProperty('--activity-types', total);
    set(panel, 'hidden', total === 0);
  }
  window.aoeActivity = { update };
}
