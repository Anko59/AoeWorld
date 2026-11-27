import { attribute, set } from '/interface/dom/update.js';
import { byId, mountPanel, renderFields } from '/interface/menu/render.js';
import { sliders } from '/interface/menu/slider.js';
import { entityIcon } from '/interface/ui-art.js';

const send = code => window.aoeBuildings?.command?.(18, code);
// Rust projects the server's boundary segments onto the detailed terrain.
export function initSteward() {
  const ns = 'http://www.w3.org/2000/svg';
  const layer = document.createElementNS(ns, 'svg');
  layer.id = 'steward-domain';
  layer.setAttribute('aria-label', 'Steward and Vassal walking zones');
  layer.style.cssText = 'position:fixed;inset:0;width:100%;height:100%;pointer-events:none;z-index:2;overflow:hidden;display:none';
  document.body.append(layer);
  const outlines = [];
  function outline(i) {
    if (!outlines[i]) {
      const node = document.createElementNS(ns, 'path');
      node.setAttribute('fill', 'none');
      node.setAttribute('vector-effect', 'non-scaling-stroke');
      layer.append(node);
      outlines[i] = { node, path: '' };
    }
    return outlines[i];
  }
  mountPanel('steward-status');
  const controls = mountPanel('steward-controls', `
    <label><input id="steward-training" type="checkbox">Train villagers and fishing ships</label>
    <button id="steward-dismiss" type="button" title="Kill idle own villagers in this territory that no Steward manages">Dismiss idle villagers</button>
  `);
  const fields = ['Food weight', 'Wood weight', 'Gold weight', 'Stone weight', 'Villager cap']
    .map((label, i) => ({ label, id: `steward-setting-${i}` }));
  const weights = sliders(controls, fields);
  const toggle = byId('steward-training');
  let signature = '', confirmed = [], pending;
  controls.addEventListener('change', () => {
    // Restore the last confirmed weights when a change would leave no active resource.
    if (weights.slice(0, 4).every(control => Number(control.input.value) === 0)) {
      confirmed.forEach((value, i) => weights[i].sync(value, true));
    } else {
      pending = { id: signature.split(',')[0], values: window.aoeSteward.settings().join(','), at: performance.now() };
      send(0);
    }
  });
  byId('steward-dismiss').addEventListener('click', () => send(1));
  window.aoeSteward = {
    settings() { return Uint8Array.from([...weights.map(control => control.input.value), Number(toggle.checked)]); },
    update(values, label, settings) {
      renderFields({
        'steward-status': { hidden: !label, textContent: label, title: label },
        'steward-controls': { hidden: !settings.length },
      });
      const next = settings.join(',');
      // Each order comes back a few frames later, so an echo of an earlier one
      // can arrive after a newer edit. Until the last order's own echo (or,
      // if the server refused it, for 3 s), the controls keep the player's values.
      if (pending && (String(settings[0]) !== pending.id || settings.slice(1).join(',') === pending.values
        || performance.now() - pending.at > 3000)) pending = undefined;
      if (pending) return updateDomain(values);
      if (next !== signature && settings.length === 7) {
        confirmed = Array.from(settings.slice(1, 5));
        weights.forEach((control, i) => control.sync(settings[i + 1], true));
        set(toggle, 'checked', settings[6] === 1);
      }
      signature = next;
      updateDomain(values, settings.length > 0);
    },
  };
  function updateDomain(values, active) {
    const count = values[0] ?? 0;
    set(layer.style, 'display', count ? 'block' : 'none');
    attribute(layer, 'data-count', count);
    attribute(layer, 'data-active', active);
    let at = 1;
    for (let i = 0; i < count; i++) {
      const vassal = values[at++] === 1, selected = values[at++] === 1, n = values[at++];
      const r = outline(i);
      set(r.node.style, 'display', '');
      attribute(r.node, 'stroke', vassal ? '#df9dff' : '#eac24d');
      attribute(r.node, 'stroke-width', selected ? '2.5' : '1.5');
      attribute(r.node, 'data-office', vassal ? 'vassal' : 'steward');
      attribute(r.node, 'data-selected', selected);
      const segments = [];
      for (let k = 0; k < n; k++, at += 4) {
        segments.push(`M${values[at]},${values[at + 1]}L${values[at + 2]},${values[at + 3]}`);
      }
      const next = segments.join('');
      if (next !== r.path) { r.node.setAttribute('d', next); r.path = next; }
    }
    for (const extra of outlines.slice(count)) set(extra.node.style, 'display', 'none');
  }
}

const focuses = ['Fortify', 'Roads'];  // Defences (towers, walls) first, or roads first.
const jobGroups = ['Current', 'Planned', 'Finished'];

export function initVassal() {
  const panel = mountPanel('vassal-panel', `
    <label id="vassal-chooser" hidden>Vassal <select aria-label="Vassal"></select></label>
    <p id="vassal-status" role="status"></p>
    <ul id="vassal-jobs" aria-label="Vassal jobs"></ul>
    <button id="vassal-development" type="button"></button>
    <ul id="vassal-squads" aria-label="Defence squads" hidden></ul>`);
  const select = panel.querySelector('select');
  const [focus] = sliders(panel, [{
    label: 'Builds first', name: 'Development priority', max: 1,
    hint: 'Choose what the Vassal builds first: defences or roads.', format: value => focuses[value], change: value => send(3 + value),
  }]);
  byId('vassal-status').after(focus.row);
  const details = Object.assign(document.createElement('div'), { id: 'vassal-job-details', hidden: true });
  details.setAttribute('role', 'tooltip');
  document.body.append(details);
  const list = byId('vassal-jobs');
  const squadList = byId('vassal-squads');
  let current, signature, roster, hovered, focused;
  const showDetails = () => {
    const row = hovered ?? focused;
    details.hidden = panel.hidden || !row?.dataset.details;
    if (details.hidden) return;
    const box = panel.getBoundingClientRect();
    details.style.right = `${innerWidth - box.right}px`;
    details.style.bottom = `${innerHeight - box.top + 6}px`;
    details.textContent = row.dataset.details;
  };
  const rows = jobGroups.map(group => {
    const row = document.createElement('li');
    row.dataset.jobs = group.toLowerCase();
    row.tabIndex = 0;
    row.addEventListener('pointerenter', () => { hovered = row; showDetails(); });
    row.addEventListener('pointerleave', () => { hovered = undefined; showDetails(); });
    row.addEventListener('focus', () => { focused = row; showDetails(); });
    row.addEventListener('blur', () => { focused = undefined; showDetails(); });
    list.append(row);
    return row;
  });
  byId('vassal-development').addEventListener('click', () => send(2));
  window.aoeVassal = {
    selected() { return panel.hidden ? 0 : Number(select.value); },
    update(values, label, jobs = '', squads = '') {
      renderFields({ 'vassal-panel': { hidden: !values.length } });
      if (!values.length) {
        current = signature = hovered = focused = undefined;
        details.hidden = true;
        return;
      }
      const [id, enabled, alert, focusValue, ...rest] = values;
      const counts = rest.slice(0, 3), occupants = rest.slice(3);
      const next = occupants.join(',');
      if (roster !== next) {
        roster = next;
        select.replaceChildren(...occupants.map((id, i) => new Option(String(i + 1), String(id))));
      }
      renderFields({ 'vassal-chooser': { hidden: occupants.length <= 1 } });
      if (select.value !== String(id)) select.value = String(id);
      focus.sync(focusValue, id !== current);
      current = id;
      const key = `${id}:${enabled}:${alert}:${label}:${counts}:${jobs}:${squads}`;
      if (signature === key) return;
      signature = key;
      renderFields({
        'vassal-panel': { 'data-alert': alert === 1 },
        'vassal-status': { textContent: label },
        'vassal-development': { textContent: enabled === 1 ? 'Stop development' : 'Resume development',
          'aria-pressed': enabled === 0 },
      });
      const lines = jobs.split('\n');
      rows.forEach((row, i) => {
        const entries = (lines[i] || '').split('\t').filter(Boolean);
        const count = counts[i] || 0;
        const heading = `${jobGroups[i]} ${count}`;
        row.textContent = count >= 10_000 ? `${jobGroups[i]} ${Math.floor(count / 1000)}k+` : heading;
        const names = entries.map(entry => entry.replace(/([a-z])([A-Z])/g, '$1 $2'));
        for (const entry of entries) {
          const token = document.createElement('span');
          token.className = 'vassal-job';
          token.dataset.job = entry;
          token.innerHTML = entityIcon(entry.split(' at ')[0]);
          row.append(token);
        }
        row.dataset.details = names.length ? [heading, ...names].join('\n') : '';
        row.setAttribute('aria-label', [heading, ...names].join(', '));
      });
      renderFields({ 'vassal-squads': { hidden: squads.length === 0 } });
      squadList.replaceChildren(...squads.split('\n').filter(Boolean).map(text => {
        const row = document.createElement('li');
        row.title = row.ariaLabel = text;
        row.textContent = text.replace(/^(\w{1,3})\w* · /, '$1 ').replace(/ (attack|mov|hold)\w*/g, ' $1');
        return row;
      }));
      showDetails();
    },
  };
}
