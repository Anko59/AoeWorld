import { trainingKinds, panelArt } from '/interface/ui-art.js';

export function initProduction() {
  const form = document.createElement('section');
  form.id = 'units-here';
  form.hidden = true;
  form.setAttribute('aria-label', 'Production destination');
  const name = kind => kind.replace(/([a-z])([A-Z])/g, '$1 $2');
  form.innerHTML = `<label>Unit type <select aria-label="Unit type">${trainingKinds.map((kind, id) => `<option value="${id}" ${id === 2 ? 'selected' : ''}>${name(kind)}</option>`).join('')}</select></label>
    <label>Unit count <input aria-label="Unit count" type="number" min="1" max="50" step="1" value="5"></label>
    <button type="button" aria-pressed="false">Units here</button>
    <p id="units-here-status" role="status">Choose a unit and count, then set a destination.</p>`;
  document.body.append(form);
  const kind = form.querySelector('select'), count = form.querySelector('input');
  const button = form.querySelector('button'), status = form.querySelector('p');
  let armed = false, visible = false, lastCount = 0, lastKind = '';
  const request = new Float64Array(2);
  function cancel(message) {
    armed = false; button.setAttribute('aria-pressed', 'false');
    document.documentElement.classList.remove('units-here-armed');
    if (message) status.textContent = message;
  }
  button.addEventListener('click', () => {
    if (armed) { cancel('Order cancelled.'); return; }
    const total = Number(count.value), unit = Number(kind.value);
    if (!Number.isInteger(total) || total < 1 || total > 50 || !trainingKinds[unit]) {
      status.textContent = 'Choose a whole count from 1 to 50.'; count.focus(); return;
    }
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
  return {
    show(value) { visible = value; form.hidden = !value; if (!value) cancel(); },
    ready(value) { panelArt(form, 'selection'); button.disabled = !value; if (!value) cancel(); },
    take() { if (!visible || !armed) return null; cancel(); return request; },
    result(total) {
      status.textContent = total ? `Requested ${total} of ${lastCount} ${lastKind}.`
        : 'No ready production, resources or population space for this request.';
    },
    reset() { cancel(); status.textContent = 'Choose a unit and count, then set a destination.'; },
  };
}
