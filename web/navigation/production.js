import { set } from '/interface/dom/update.js';
import { names, trainingKinds } from '/interface/menu/catalog.js';
import { commandButtons } from '/interface/menu/render.js';
import { artReady } from '/interface/ui-art.js';

// "Units here": set a destination once, choose a count, then click any number
// of units; each click trains that many at the nearest eligible producers and
// sends them to the destination. Rust picks the producers from all owned
// buildings, so nothing here depends on the building selection. Units that no
// owned building can train, or that an upgrade replaced, are not listed.
export const reasons = ['', '', 'Feudal Age', '', 'Researching', 'Busy', 'Food', 'Wood', 'Gold', 'Stone', 'Queue full', 'Under construction', 'Needs building', '', 'Castle Age', 'Unique', 'Imperial Age'];
export function initProduction() {
  const form = document.createElement('section');
  form.id = 'units-here';
  form.setAttribute('aria-label', 'Production destination');
  form.innerHTML = `<div class="training-order">
    <div class="far-row"><button type="button" class="far-act" id="units-here-destination" aria-pressed="false">Set destination</button></div>
    <div class="far-row" role="group" aria-label="Batch size"><label>Count <input aria-label="Unit count" class="far-input" type="number" min="1" max="10" step="1" value="5"></label><span class="far-counts"></span></div>
    <p id="units-here-status" class="far-status" role="status">Set a destination, choose a count, then click units.</p>
    </div><div class="far-grid" role="group" aria-label="Unit type"></div>`;
  let codes = [], destination = false, queue = [];
  const count = form.querySelector('input'), status = form.querySelector('p');
  const setDestination = form.querySelector('#units-here-destination');
  setDestination.setAttribute('aria-label', 'Set destination');
  const render = commandButtons(), sizes = [1, 5, 10];
  const roster = form.querySelector('.far-grid');
  const options = trainingKinds.map((kind, id) => ({
    id: 'units-here-unit-' + id, label: names[kind], icon: ['entity', kind], className: 'far-cell',
  }));
  const presets = sizes.map(n => ({ id: 'units-here-size-' + n, label: String(n), className: 'far-chip' }));
  let armed = false, visible = false, busy = false, available = false, lastCount = 0, lastKind = '';
  // Code 0: nothing can train it now (an upgrade replaced it); 12: no owned
  // building can. Both stay out of the list.
  const listed = id => codes.length === 0 || (codes[id] !== 0 && codes[id] !== 12);
  const cells = options.map((option, id) => {
    const cell = render({ ...option, name: option.label });
    cell.dataset.id = String(id);
    cell.addEventListener('click', () => order(id));
    return cell;
  });
  function drawRoster() {
    options.forEach((option, id) => {
      const note = codes.length > 0 && codes[id] !== 1 ? reasons[codes[id]] ?? '' : '';
      render({ ...option, name: option.label, disabled: !destination || !available || (codes.length > 0 && codes[id] !== 1), note, reason: note });
    });
    const shown = cells.filter((_, id) => listed(id));
    if (shown.length !== roster.children.length || shown.some((cell, at) => roster.children[at] !== cell)) roster.replaceChildren(...shown);
  }
  function drawAction() {
    set(setDestination, 'textContent', armed ? 'Click the ground…' : destination ? 'Change destination' : 'Set destination');
    set(setDestination, 'disabled', !available);
    setDestination.setAttribute('aria-pressed', String(armed));
    drawRoster();
  }
  const presetButton = preset => render({ ...preset, pressed: preset.label === count.value });
  function drawOrder() { presets.forEach(presetButton); }
  count.addEventListener('input', drawOrder);
  form.querySelector('.far-counts').append(...presets.map(preset => {
    const chip = presetButton(preset);
    chip.addEventListener('click', () => { set(count, 'value', preset.label); drawOrder(); });
    return chip;
  }));
  artReady.then(drawAction);
  drawAction();
  const request = new Float64Array(2);
  function cancel(message) {
    armed = false;
    document.documentElement.classList.remove('units-here-armed');
    if (message) status.textContent = message;
    drawAction();
  }
  // One click sends the whole batch to the stored destination; clicks made
  // while the previous batch is in flight wait their turn.
  function send(unit, total) {
    busy = true;
    lastCount = total; lastKind = options[unit].label;
    window.aoeBuildings?.command(19, unit, 0, total);
  }
  function next() {
    const pending = queue.shift();
    if (pending) send(...pending);
  }
  function order(unit) {
    const total = Number(count.value);
    if (!destination || !Number.isInteger(total) || total < 1 || total > 10 || !trainingKinds[unit]) {
      status.textContent = destination ? 'Choose a whole count from 1 to 10.' : 'Set a destination first.'; return;
    }
    if (busy) { queue.push([unit, total]); status.textContent = `Queued ${total} ${options[unit].label}.`; return; }
    send(unit, total);
  }
  setDestination.addEventListener('click', () => {
    if (armed) { cancel('Order cancelled.'); return; }
    window.aoeBuildings?.command(11, 0);
    request.set([-1, 0]);
    armed = true; drawAction();
    document.documentElement.classList.add('units-here-armed');
    status.textContent = 'Click ground in the far view · Esc or right-click cancels.';
    document.getElementById('scene')?.focus();
  });
  document.addEventListener('keydown', event => {
    if (armed && event.key === 'Escape') { event.preventDefault(); cancel('Order cancelled.'); }
  });
  document.addEventListener('pointerdown', event => {
    if (armed && event.button === 2 && event.target instanceof Element && event.target.closest('#scene,#minimap-map')) {
      event.preventDefault(); event.stopImmediatePropagation(); cancel('Order cancelled.');
    }
  }, true);
  window.addEventListener('blur', () => cancel());
  return {
    form,
    show(value) { visible = value; if (!value) cancel(); },
    ready(value) { available = value; if (!value) { busy = false; queue = []; cancel(); } else drawAction(); },
    units(values) {
      if (codes.length === values.length && codes.every((code, id) => code === values[id])) return;
      codes = Array.from(values);
      drawAction();
    },
    take() { if (!visible || !armed) return null; cancel(); return request; },
    placed(ok) {
      destination = destination || ok;
      status.textContent = ok ? 'Destination set. Click units to train them there.' : 'That ground is outside the map.';
      drawAction();
    },
    result(total) {
      busy = total > 0;
      status.textContent = total ? `Submitting ${total} of ${lastCount} ${lastKind}…`
        : 'No ready production, resources or population space for this request.';
      if (!busy) next();
      drawAction();
    },
    accepted(total, done) {
      busy = !done;
      status.textContent = `Accepted ${total} of ${lastCount} ${lastKind}.${done ? '' : ' Waiting for server…'}`;
      if (done) next();
    },
    reset() { busy = false; queue = []; destination = false; cancel('Set a destination, choose a count, then click units.'); },
  };
}
