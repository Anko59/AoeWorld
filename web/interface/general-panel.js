// The selected General's panel. Authoritative values arrive from Rust through
// window.aoeGeneral.update(values); orders leave through the shared callback
// (action 9: Train on/off; action 10: mix slider, role*128+percent).
import { squadIcon } from '/interface/ui-art.js';
import { byId, mountPanel, renderFields, show } from '/interface/menu/render.js';
import { sliders } from '/interface/menu/slider.js';
// values = [] to hide, else [id, army, state, training, pending,
//           mix0..mix4, count0..count4, squads, (leader, kind, size, state)*squads].
const roles = ['Infantry', 'Archers', 'Cavalry', 'Siege', 'Scouts'];
const states = ['idle', 'marching', 'fighting', 'retreating'];
const modes = new Map(); // General id -> 'captain' | 'trainer'; a new General starts as captain.

export function initGeneralPanel() {
  // The Rust callback is stored by buildings.js when the client binds; read it at click time.
  const send = (action, code) => window.aoeBuildings?.command?.(action, code);
  mountPanel('general-panel', `<div id="general-modes" role="tablist">
      <button type="button" id="general-captain" role="tab" data-mode="captain">Captain</button>
      <button type="button" id="general-trainer" role="tab" data-mode="trainer">Trainer</button></div>
    <div id="general-captain-view"><div id="general-submodes">
      <span id="general-marching" class="submode">Marching</span>
      <span id="general-fighting" class="submode">Fighting</span></div>
      <p id="general-army"></p><ul id="general-squads" aria-label="Squads"></ul></div>
    <div id="general-trainer-view"><div id="general-sliders"></div>
      <button type="button" id="general-train"></button><p id="general-train-note"></p></div>`);
  const mix = sliders(byId('general-sliders'), roles.map((label, role) => ({
    label, name: label + ' share', step: 5, suffix: '%',
    input: value => send(10, role * 128 + value),
  })));
  let current = null, training = false, roster = '';
  function renderMode(mode) {
    renderFields({
      'general-panel': { 'data-mode': mode },
      'general-captain': { 'aria-selected': mode === 'captain' },
      'general-trainer': { 'aria-selected': mode === 'trainer' },
      'general-captain-view': { hidden: mode !== 'captain' },
      'general-trainer-view': { hidden: mode !== 'trainer' },
    });
  }
  byId('general-modes').addEventListener('click', event => {
    const mode = event.target.closest('button')?.dataset.mode;
    if (current === null || !mode) return;
    modes.set(current, mode);
    renderMode(mode);
  });
  byId('general-train').addEventListener('click', () => send(9, Number(!training)));
  window.aoeGeneral = {
    update(values) {
      if (!values?.length) {
        current = null;
        show('general-panel', false);
        return;
      }
      const [id, army, state, isTraining, pending] = values;
      const changed = id !== current, posture = states[state] ?? 'idle';
      current = id;
      training = isTraining === 1;
      renderMode(modes.get(id) ?? 'captain');
      renderFields({
        'general-panel': { hidden: false, 'data-general': id, 'data-state': posture },
        'general-marching': { 'aria-current': posture === 'marching' },
        'general-fighting': { 'aria-current': posture === 'fighting' },
        'general-army': { textContent: `${army} soldiers` },
        'general-train': { 'aria-pressed': training, textContent: training ? 'Stop training' : 'Train' },
        'general-train-note': { textContent: training
          ? `Training squad vacancies (${pending} queued)`
          : 'Idle · press Train to fill squad vacancies' },
      });
      const signature = `${id}:${values.slice(15).join(',')}`;
      if (signature !== roster) {
        roster = signature;
        const capacities = [20, 15, 10, 5], squads = [];
        for (let n = 0; n < Math.min(10, values[15]); n++) {
          const [leader, kind, size, status] = values.slice(16 + n * 4, 20 + n * 4);
          squads.push(`<li data-leader="${leader}">${squadIcon(kind)}<span>${roles[kind]} <b>${size}/${capacities[kind]}</b><small>${states[status]}</small></span></li>`);
        }
        renderFields({ 'general-squads': { innerHTML: squads.join('') || '<li>No squads yet</li>' } });
      }
      mix.forEach((control, role) => control.sync(values[5 + role], changed));
    },
  };
}
