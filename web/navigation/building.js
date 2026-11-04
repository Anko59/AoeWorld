import { buildingKinds, buildingNames } from '/interface/buildings.js';
import { entityIcon } from '/interface/ui-art.js';

export function initFarBuilding(panel) {
  const form = document.createElement('section');
  form.id = 'far-building';
  form.setAttribute('aria-label', 'Build from kingdom view');
  form.innerHTML = `<label>Building <select aria-label="Building type">${buildingNames.map((name, id) => `<option value="${id}"${id === 1 ? ' selected' : ''}>${name}</option>`).join('')}</select></label>
    <button type="button">Build here</button><p role="status">Choose a building, then a site. Nearby free villagers answer.</p>`;
  panel.append(form);
  const select = form.querySelector('select'), button = form.querySelector('button'), status = form.querySelector('p');
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.id = 'far-footprint'; svg.setAttribute('hidden', ''); svg.setAttribute('aria-hidden', 'true');
  document.body.append(svg);
  let visible = false, state = [], signature = '', shownKind = -1;
  function refresh() {
    const kind = Number(select.value);
    button.disabled = state[(kind + buildingKinds.length - 1) % buildingKinds.length] !== 1;
    if (shownKind !== kind) {
      shownKind = kind;
      button.innerHTML = `${entityIcon(buildingKinds[kind])} Build here`;
    }
    button.title = button.disabled ? 'Needs available villagers, resources, age and prerequisite buildings' : 'Choose a site in the far view';
  }
  select.addEventListener('change', refresh);
  button.addEventListener('click', () => {
    window.aoeNavigation?.production?.reset();
    window.aoeBuildings?.command(0, (Number(select.value) + buildingKinds.length - 1) % buildingKinds.length);
    status.textContent = 'Click a clear site · free villagers within 64 tiles answer · Esc or right-click cancels.';
    document.getElementById('scene')?.focus();
  });
  return {
    ready(values) { state = Array.from(values); refresh(); },
    show(value) { visible = value; if (!value) svg.setAttribute('hidden', ''); },
    preview(values, reason) {
      svg.toggleAttribute('hidden', !visible || values.length < 9);
      const next = Array.from(values).join(',');
      if (next !== signature) {
        signature = next;
        svg.replaceChildren();
        for (let at = 1; at + 7 < values.length; at += 8) {
          const polygon = document.createElementNS(svg.namespaceURI, 'polygon');
          polygon.setAttribute('points', Array.from(values.slice(at, at + 8)).reduce((s, n, i) => s + (i % 2 ? ',' : ' ') + n, '').trim());
          svg.append(polygon);
        }
        svg.dataset.valid = String(values[0] === 1);
      }
      status.dataset.invalid = String(values.length > 0 && values[0] !== 1);
      if (values.length && status.textContent !== reason) status.textContent = reason;
    },
    result(message) { status.dataset.invalid = String(!message.startsWith('Accepted') && !message.startsWith('Submitting')); status.textContent = message; },
    reset() { svg.setAttribute('hidden', ''); signature = ''; status.textContent = 'Choose a building, then a site. Nearby free villagers answer.'; },
  };
}
