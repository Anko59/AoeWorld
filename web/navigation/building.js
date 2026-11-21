import { buildingKinds, buildingNames } from '/interface/buildings.js';
import { entityIcon } from '/interface/ui-art.js';
import { iconGrid } from '/navigation/cockpit/grid.js';

// Build from far zoom: one click on a building icon arms its footprint, the
// next click on the map places it. No drop-down; availability is Rust's.
const prompt = 'Choose a building, then place its foundation. Nearby villagers build as capacity opens.';
export function initFarBuilding(panel) {
  const form = document.createElement('section');
  form.id = 'far-building';
  form.setAttribute('aria-label', 'Build from kingdom view');
  form.innerHTML = `<div class="far-slot"></div><p role="status" class="far-status">${prompt}</p>`;
  panel.append(form);
  // Guard Tower and Keep are upgrades, not foundations: the build commands
  // and availability codes cover only the kinds a villager can place.
  const buildable = buildingKinds.filter(kind => kind !== 'GuardTower' && kind !== 'Keep');
  const count = buildable.length;
  const grid = iconGrid('Building type', buildable.map((kind, id) => ({ id, label: buildingNames[id], art: entityIcon(kind) })), id => {
    window.aoeNavigation?.production?.reset();
    window.aoeBuildings?.command(0, (id + count - 1) % count);
    chosen = id; mark();
    status.textContent = 'Click a clear site · nearby villagers build when available · Esc or right-click cancels.';
    document.getElementById('scene')?.focus();
  });
  form.querySelector('.far-slot').append(grid.grid);
  const status = form.querySelector('p');
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.id = 'far-footprint'; svg.setAttribute('hidden', ''); svg.setAttribute('aria-hidden', 'true');
  document.body.append(svg);
  let visible = false, state = [], signature = '', chosen = -1;
  const mark = () => buildable.forEach((_, id) => grid.set(id, { selected: id === chosen }));
  return {
    ready(values) {
      state = Array.from(values);
      buildable.forEach((_, id) => {
        const ok = state[(id + count - 1) % count] === 1;
        grid.set(id, { enabled: ok, title: `${buildingNames[id]}${ok ? '' : ' · needs resources, the required age and prerequisite buildings'}` });
      });
    },
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
    reset() { svg.setAttribute('hidden', ''); signature = ''; chosen = -1; mark(); status.textContent = prompt; },
  };
}
