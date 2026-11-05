// The selected General's panel. Authoritative values arrive from Rust through
// window.aoeGeneral.update(values); orders leave through the shared callback
// (action 9: Train on/off; action 10: mix slider, role*128+percent).
import { squadIcon } from '/interface/ui-art.js';
// values = [] to hide, else [id, army, state, training, pending,
//           mix0..mix4, count0..count4, squads, (leader, kind, size, state)*squads].
const roles = ['Infantry', 'Archers', 'Cavalry', 'Siege', 'Scouts'];
const states = ['idle', 'marching', 'fighting', 'retreating'];
const modes = new Map(); // General id -> 'captain' | 'trainer'; a new General starts as captain.

export function initGeneralPanel() {
  // The Rust callback is stored by buildings.js when the client binds; read it at click time.
  const send = (action, code) => window.aoeBuildings?.command?.(action, code);
  const panel = document.createElement('div');
  panel.id = 'general-panel'; panel.hidden = true;
  panel.innerHTML = `<div id="general-modes" role="tablist">
      <button type="button" id="general-captain" role="tab" data-mode="captain">Captain</button>
      <button type="button" id="general-trainer" role="tab" data-mode="trainer">Trainer</button></div>
    <div id="general-captain-view"><div id="general-submodes">
      <span id="general-marching" class="submode">Marching</span>
      <span id="general-fighting" class="submode">Fighting</span></div>
      <p id="general-army"></p><ul id="general-squads" aria-label="Squads"></ul></div>
    <div id="general-trainer-view"><div id="general-sliders"></div>
      <button type="button" id="general-train"></button><p id="general-train-note"></p></div>`;
  (document.getElementById('selection-content') ?? document.body).append(panel);
  const byId = id => panel.querySelector(`#${id}`);
  const sliders = roles.map((name, role) => {
    const row = document.createElement('label'); row.className = 'general-slider';
    row.innerHTML = `<span class="role">${name}</span><input type="range" min="0" max="100" step="5" aria-label="${name} share"><output></output>`;
    const input = row.querySelector('input');
    input.addEventListener('input', () => { input.style.setProperty('--fill', `${input.value}%`); row.querySelector('output').textContent = `${input.value}%`; send(10, role * 128 + Number(input.value)); });
    byId('general-sliders').append(row);
    return { input, out: row.querySelector('output') };
  });
  let current = null, training = false, roster = '';
  const setMode = mode => { if (current === null) return; modes.set(current, mode); render(mode); };
  const render = mode => {
    panel.dataset.mode = mode;
    for (const m of ['captain', 'trainer']) {
      byId(`general-${m}`).setAttribute('aria-selected', String(m === mode));
      byId(`general-${m}-view`).hidden = m !== mode;
    }
  };
  byId('general-captain').addEventListener('click', () => setMode('captain'));
  byId('general-trainer').addEventListener('click', () => setMode('trainer'));
  byId('general-train').addEventListener('click', () => send(9, training ? 0 : 1));
  window.aoeGeneral = {
    update(values) {
      if (!values || values.length === 0) { panel.hidden = true; current = null; return; }
      const [id, army, state, isTraining, pending] = values;
      const changed = id !== current;
      current = id;
      panel.dataset.general = String(id);
      panel.hidden = false;
      render(modes.get(id) ?? 'captain');
      training = isTraining === 1;
      panel.dataset.state = states[state] ?? 'idle';
      for (const s of ['marching', 'fighting']) byId(`general-${s}`).setAttribute('aria-current', String(states[state] === s));
      byId('general-army').textContent = `${army} soldiers · up to 10 squads · ${states[state] ?? 'idle'}`;
      const signature = `${id}:${values.slice(15).join(',')}`;
      if (signature !== roster) {
        roster = signature;
        const list = byId('general-squads'); list.replaceChildren();
        const capacities = [20,15,10,5];
        for (let n = 0; n < Math.min(10, values[15]); n++) {
          const [leader, kind, size, status] = values.slice(16 + n * 4, 20 + n * 4);
          const li = document.createElement('li'); li.dataset.leader = String(leader);
          li.innerHTML = `${squadIcon(kind)}<span>${roles[kind]} <b>${size}/${capacities[kind]}</b><small>${states[status]}</small></span>`;
          list.append(li);
        }
        if (values[15] === 0) { const li = document.createElement('li'); li.textContent = 'No squads yet'; list.append(li); }
      }
      sliders.forEach((s, role) => {
        // Never fight the hand that is dragging a slider.
        if (changed || document.activeElement !== s.input) s.input.value = String(values[5 + role]);
        s.out.textContent = `${s.input.value}%`;
        s.input.style.setProperty('--fill', `${s.input.value}%`);
      });
      const button = byId('general-train');
      button.textContent = training ? 'Stop training' : 'Train';
      button.setAttribute('aria-pressed', String(training));
      byId('general-train-note').textContent = training
        ? `Training squad vacancies (${pending} queued)`
        : 'Idle · press Train to fill squad vacancies';
    },
  };
}
