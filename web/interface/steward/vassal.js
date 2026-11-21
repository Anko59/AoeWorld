import { fillRange } from '/interface/panel-frame.js';
import { entityIcon } from '/interface/ui-art.js';

const focuses = ['Fortify', 'Roads'];
const jobGroups = ['Current', 'Planned', 'Finished'];

export function initVassal() {
  const panel = document.createElement('div');
  panel.id = 'vassal-panel';
  panel.hidden = true;
  panel.innerHTML = `
    <label id="vassal-chooser" hidden>Vassal <select aria-label="Vassal"></select></label>
    <div id="vassal-controls"></div>
    <p id="vassal-status" role="status"></p>
    <ul id="vassal-jobs" aria-label="Vassal jobs"></ul>
    <button id="vassal-development" type="button"></button>
    <ul id="vassal-squads" aria-label="Defence squads" hidden></ul>`;
  document.getElementById('selection-content').append(panel);
  const status = panel.querySelector('#vassal-status');
  const toggle = panel.querySelector('#vassal-development');
  const controls = panel.querySelector('#vassal-controls');
  const list = panel.querySelector('#vassal-jobs');
  const squadsList = panel.querySelector('#vassal-squads');
  const details = document.createElement('div');
  details.id = 'vassal-job-details';
  details.setAttribute('role', 'tooltip');
  details.hidden = true;
  document.body.append(details);
  const chooser = panel.querySelector('#vassal-chooser');
  const select = chooser.querySelector('select');
  let send, current, signature, roster;
  const updateChooser = (id, occupants) => {
    const next = occupants.join(',');
    if (roster !== next) {
      roster = next;
      chooser.hidden = occupants.length <= 1;
      select.replaceChildren(...occupants.map((id, i) => new Option(String(i + 1), String(id))));
    }
    if (select.value !== String(id)) select.value = String(id);
  };
  const slider = (name, min, max, action, format) => {
    const row = document.createElement('label');
    row.className = 'general-slider';
    const label = document.createElement('span');
    label.textContent = name;
    const input = document.createElement('input');
    Object.assign(input, { type: 'range', min, max, step: 1 });
    input.setAttribute('aria-label', name);
    const output = document.createElement('output');
    const draw = () => {
      const text = format(Number(input.value));
      output.textContent = text;
      input.setAttribute('aria-valuetext', text);
      fillRange(input);
    };
    input.addEventListener('input', draw);
    input.addEventListener('change', () => send?.(action, Number(input.value)));
    row.append(label, input, output);
    controls.append(row);
    return (value, changed) => {
      if (changed || (document.activeElement !== input && Number(input.value) !== value)) {
        input.value = String(value);
        draw();
      }
    };
  };
  const focus = slider('Focus', 0, 1, 1, value => focuses[value]);
  let hovered, focused;
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
  toggle.addEventListener('click', () => send?.(0, 0));

  window.aoeVassal = {
    bind(callback) { send = callback; },
    selected() { return panel.hidden ? 0 : Number(select.value); },
    update(values, label, jobs, squads) {
      panel.hidden = values.length === 0;
      if (panel.hidden) {
        current = hovered = focused = undefined;
        details.hidden = true;
        return;
      }
      const [id, enabled, alert, focusValue, ...rest] = values;
      const counts = rest.slice(0, 3);
      updateChooser(id, rest.slice(3));
      const changed = id !== current;
      current = id;
      focus(focusValue, changed);
      const next = `${id}:${enabled}:${alert}:${label}:${counts.join(',')}:${jobs}:${squads}`;
      if (signature === next) return;
      signature = next;
      panel.dataset.alert = String(alert === 1);
      status.textContent = label;
      toggle.textContent = enabled === 1 ? 'Stop development' : 'Resume development';
      toggle.setAttribute('aria-pressed', String(enabled === 0));
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
          const kind = entry.split(' at ')[0];
          token.innerHTML = entityIcon(kind);
          row.append(token);
        }
        row.dataset.details = names.length ? [heading, ...names].join('\n') : '';
        row.setAttribute('aria-label', [heading, ...names].join(', '));
      });
      squadsList.hidden = squads.length === 0;
      squadsList.replaceChildren(...squads.split('\n').filter(Boolean).map(text => {
        const row = document.createElement('li');
        row.textContent = text;
        return row;
      }));
      showDetails();
    },
  };
}
