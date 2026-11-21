import { set } from '/interface/dom/update.js';
import { names, trainingKinds } from '/interface/menu/catalog.js';
import { commandButtons } from '/interface/menu/render.js';
import { artReady } from '/interface/ui-art.js';

// "Units here": choose a unit from an icon grid and a batch size, then click
// ground. Rust picks the nearest eligible producers from all owned buildings, so
// nothing here depends on the building selection. Every trainable unit is listed;
// Rust's selection-independent report (units()) says which one is ready and why not.
export const reasons = ['', '', 'Feudal Age', '', 'Researching', 'Busy', 'Food', 'Wood', 'Gold', 'Stone', 'Queue full', 'Under construction', 'Needs building', '', 'Castle Age', 'Unique', 'Imperial Age'];
export function initProduction() {
  const form = document.createElement('section');
  form.id = 'units-here';
  form.setAttribute('aria-label', 'Production destination');
  form.innerHTML = `<div class="training-order">
    <div class="far-row" role="group" aria-label="Batch size"><label>Count <input aria-label="Unit count" class="far-input" type="number" min="1" max="10" step="1" value="5"></label><span class="far-counts"></span></div>
    <p id="units-here-status" class="far-status" role="status">Choose a unit and count, then set a destination.</p>
    </div><div class="far-grid" role="group" aria-label="Unit type"></div>`;
  let chosen = 2, codes = [];
  const count = form.querySelector('input'), status = form.querySelector('p');
  const render = commandButtons(), sizes = [1, 5, 10];
  const roster = form.querySelector('.far-grid');
  const options = trainingKinds.map((kind, id) => ({
    id: 'units-here-unit-' + id, label: names[kind], icon: ['entity', kind], className: 'far-cell',
  }));
  const presets = sizes.map(n => ({ id: 'units-here-size-' + n, label: String(n), className: 'far-chip' }));
  let armed = false, visible = false, busy = false, available = false, lastCount = 0, lastKind = '';
  function unitButton(option, id) {
    const disabled = codes.length > 0 && codes[id] !== 1;
    const note = disabled ? reasons[codes[id]] ?? '' : '';
    return render({ ...option, disabled, note, reason: note, pressed: chosen === id });
  }
  function drawAction() {
    return render({ id: 'units-here-order', name: 'Units here', className: 'far-act',
      label: `${options[chosen].label} × ${count.value} here`, pressed: armed,
      disabled: busy || !available || (codes.length > 0 && codes[chosen] !== 1 && !armed),
    });
  }
  const presetButton = preset => render({ ...preset, pressed: preset.label === count.value });
  const drawRoster = () => options.forEach(unitButton);
  function drawOrder() { presets.forEach(presetButton); drawAction(); }
  const button = drawAction();
  status.before(button);
  count.addEventListener('input', drawOrder);
  form.querySelector('.far-counts').append(...presets.map(preset => {
    const chip = presetButton(preset);
    chip.addEventListener('click', () => { set(count, 'value', preset.label); drawOrder(); });
    return chip;
  }));
  roster.append(...options.map((option, id) => {
    const cell = unitButton(option, id);
    cell.dataset.id = String(id);
    cell.addEventListener('click', () => { chosen = id; drawRoster(); drawAction(); });
    return cell;
  }));
  artReady.then(drawRoster);
  const request = new Float64Array(2);
  function cancel(message) {
    armed = false;
    document.documentElement.classList.remove('units-here-armed');
    if (message) status.textContent = message;
    drawAction();
  }
  button.addEventListener('click', () => {
    if (armed) { cancel('Order cancelled.'); return; }
    const total = Number(count.value), unit = chosen;
    if (!Number.isInteger(total) || total < 1 || total > 10 || !trainingKinds[unit]) {
      status.textContent = 'Choose a whole count from 1 to 10.'; return;
    }
    window.aoeBuildings?.command(11, 0);
    lastCount = total; lastKind = options[unit].label; request.set([unit, total]);
    armed = true; drawAction();
    document.documentElement.classList.add('units-here-armed');
    status.textContent = 'Click ground in the far view · Esc or right-click cancels.';
    document.getElementById('scene')?.focus();
  });
  document.addEventListener('keydown', event => {
    if (armed && event.key === 'Escape') { event.preventDefault(); cancel('Order cancelled.'); }
  });
  document.addEventListener('pointerdown', event => {
    if (armed && event.button === 2 && event.target === document.getElementById('scene')) {
      event.preventDefault(); event.stopImmediatePropagation(); cancel('Order cancelled.');
    }
  }, true);
  window.addEventListener('blur', () => cancel());
  return {
    form,
    show(value) { visible = value; if (!value) cancel(); },
    ready(value) { available = value; if (!value) { busy = false; cancel(); } else drawAction(); },
    units(values) {
      if (codes.length === values.length && codes.every((code, id) => code === values[id])) return;
      codes = Array.from(values);
      drawRoster();
      drawAction();
    },
    take() { if (!visible || !armed) return null; cancel(); return request; },
    result(total) {
      busy = total > 0; drawAction();
      status.textContent = total ? `Submitting ${total} of ${lastCount} ${lastKind}…`
        : 'No ready production, resources or population space for this request.';
    },
    accepted(total, done) {
      busy = !done; drawAction();
      status.textContent = `Accepted ${total} of ${lastCount} ${lastKind}.${done ? '' : ' Waiting for server…'}`;
    },
    reset() { busy = false; cancel('Choose a unit and count, then set a destination.'); },
  };
}
