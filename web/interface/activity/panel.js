import { entityIcon, icon, trainingKinds, techIds } from '/interface/ui-art.js';
import { set, attribute } from '/interface/dom/update.js';

export function initActivity() {
  const panel = document.createElement('section');
  panel.id = 'kingdom-activity'; panel.hidden = true;
  panel.setAttribute('aria-label', 'Kingdom activity');
  panel.innerHTML = '<h2>Kingdom activity</h2><div class="activity-rows"></div><small class="activity-more" hidden></small>';
  document.getElementById('playground').append(panel);
  const host = panel.querySelector('.activity-rows'), more = panel.querySelector('.activity-more');
  const rows = new Map();
  let latest = [], limit = 6;
  function update(values) {
    latest = Array.from(values);
    const live = new Set(), total = Math.max(0, Math.floor((latest.length - 1) / 5));
    for (let i = 0; i < Math.min(total, limit); i++) {
      const [research, kind, count, progress, building] = latest.slice(1 + i * 5, 6 + i * 5);
      const key = `${research}:${kind}`;
      live.add(key);
      let row = rows.get(key);
      const name = research ? window.aoeBuildings.techs[kind].label.split(' · ')[0] : trainingKinds[kind].replace(/([a-z])([A-Z])/g, '$1 $2');
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
    const overflow = (latest[0] ?? 0) + Math.max(0, total - limit);
    set(more, 'textContent', `+${overflow} more`); set(more, 'hidden', !overflow);
    set(panel, 'hidden', total === 0);
  }
  function resize() {
    const bottom = parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--bottom')) || 208;
    limit = Math.max(1, Math.min(6, Math.floor((innerHeight - bottom - 104) / 34)));
    update(latest);
  }
  window.addEventListener('resize', resize);
  window.aoeActivity = { update };
  resize();
}
