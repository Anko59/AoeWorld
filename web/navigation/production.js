import { trainingKinds, entityIcon } from '/interface/ui-art.js';
import { iconGrid } from '/navigation/cockpit/grid.js';

// "Units here": choose a unit from an icon grid and a batch size, then click
// ground. Availability per unit is Rust's own report (units()).
const reasons = ['', '', 'Feudal Age', '', '', 'Busy', 'Food', 'Wood', 'Gold', 'Stone', 'Queue full', '', 'Needs building', '', 'Castle Age', 'Unique', 'Imperial Age'];
const naval = new Set(['FishingShip', 'Galley', 'FireShip', 'DemolitionShip', 'TransportShip', 'WarGalley']);
export function initProduction() {
  const form = document.createElement('section');
  form.id = 'units-here';
  form.setAttribute('aria-label', 'Production destination');
  const name = kind => kind.replace(/([a-z])([A-Z])/g, '$1 $2');
  form.innerHTML = `<div class="far-group">Unit</div><div class="far-slot"></div>
    <div class="far-row" role="group" aria-label="Unit count group"><label>Count <input aria-label="Unit count" class="far-input" style="width:56px" type="number" min="1" max="10" step="1" value="5"></label><span class="far-counts"></span></div>
    <button type="button" class="far-act" aria-pressed="false">Units here</button>
    <p id="units-here-status" class="far-status" role="status">Choose a unit and count, then set a destination.</p>`;
  let chosen = 2;
  const grid = iconGrid('Unit type', trainingKinds.flatMap((kind, id) => naval.has(kind) || kind === 'Steward' || kind === 'Vassal' ? [] : [{ id, label: name(kind), art: entityIcon(kind) }]), id => {
    chosen = id; mark();
  });
  form.querySelector('.far-slot').append(grid.grid);
  const count = form.querySelector('input'), button = form.querySelector('button.far-act'), status = form.querySelector('p');
  button.setAttribute('aria-label', 'Units here');
  count.addEventListener('input', mark);
  const chips = [1, 5, 10].map(n => {
    const chip = document.createElement('button');
    chip.type = 'button'; chip.className = 'far-chip'; chip.textContent = String(n);
    chip.addEventListener('click', () => { count.value = String(n); mark(); });
    form.querySelector('.far-counts').append(chip);
    return chip;
  });
  let armed = false, visible = false, busy = false, available = false, lastCount = 0, lastKind = '', codes = [];
  const enabled = () => { button.disabled = busy || !available || (codes.length > 0 && codes[chosen] !== 1 && !armed); };
  function mark() {
    trainingKinds.forEach((kind, id) => grid.get(id) && grid.set(id, { selected: id === chosen }));
    chips.forEach(chip => chip.setAttribute('aria-pressed', String(chip.textContent === count.value)));
    button.textContent = `${name(trainingKinds[chosen] ?? 'Units')} × ${count.value} here`;
    enabled();
  }
  const request = new Float64Array(2);
  function cancel(message) {
    armed = false; button.setAttribute('aria-pressed', 'false');
    document.documentElement.classList.remove('units-here-armed');
    if (message) status.textContent = message;
    enabled();
  }
  button.addEventListener('click', () => {
    if (armed) { cancel('Order cancelled.'); return; }
    const total = Number(count.value), unit = chosen;
    if (!Number.isInteger(total) || total < 1 || total > 10 || !trainingKinds[unit]) {
      status.textContent = 'Choose a whole count from 1 to 10.'; return;
    }
    window.aoeBuildings?.command(11, 0);
    lastCount = total; lastKind = name(trainingKinds[unit]); request.set([unit, total]);
    armed = true; button.setAttribute('aria-pressed', 'true');
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
  mark();
  return {
    form,
    show(value) { visible = value; if (!value) cancel(); },
    ready(value) { available = value; if (!value) { cancel(); busy = false; } enabled(); },
    units(values) {
      codes = Array.from(values);
      trainingKinds.forEach((kind, id) => grid.get(id) && grid.set(id, { enabled: codes[id] === 1 || codes.length === 0,
        note: codes[id] === 1 ? '' : reasons[codes[id]] ?? '' }));
      enabled();
    },
    take() { if (!visible || !armed) return null; cancel(); return request; },
    result(total) {
      busy = total > 0; enabled();
      status.textContent = total ? `Submitting ${total} of ${lastCount} ${lastKind}…`
        : 'No ready production, resources or population space for this request.';
    },
    accepted(total, done) {
      busy = !done; enabled();
      status.textContent = `Accepted ${total} of ${lastCount} ${lastKind}.${done ? '' : ' Waiting for server…'}`;
    },
    reset() { cancel(); busy = false; enabled(); status.textContent = 'Choose a unit and count, then set a destination.'; },
  };
}
