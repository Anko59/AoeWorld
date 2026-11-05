// Fits the selected-unit icons into the room the panel has, so the selection
// never scrolls and never clips. The container's size comes from CSS and not
// from its tokens; this picks the largest square token that fits every icon.
// When even the smallest comfortable token cannot hold one icon per unit,
// identical units share one icon with a count (click selects that whole type).
const GAP = 3, BIGGEST = 72, SMALLEST = 28, TINIEST = 12;
// Largest size in [low, high] where `count` squares fit in width x height, else 0.
export function tokenSize(width, height, count, high = BIGGEST, low = SMALLEST, gap = GAP) {
  // One pixel of slack: layout widths are rounded, and a row that fits exactly would wrap.
  for (let size = high; size >= low; size--) {
    const columns = Math.floor((width - 1 + gap) / (size + gap)), rows = Math.floor((height - 1 + gap) / (size + gap));
    if (columns > 0 && rows > 0 && columns * rows >= count) return size;
  }
  return 0;
}
// units: [{ id, kind, name, tint }], icon(kind, tint) -> html.
export function groupUnits(units) {
  const groups = new Map();
  for (const unit of units) {
    const key = `${unit.kind}|${unit.tint ?? ''}`, group = groups.get(key);
    if (group) group.count++; else groups.set(key, { ...unit, count: 1 });
  }
  return [...groups.values()];
}
const states = new WeakMap();
const token = (unit, html, count) => count
  ? `<span class="unit-token unit-group" data-unit-id="${unit.id}" data-count="${count}" title="${count} ${unit.name}">${html}<b class="token-count">${count}</b></span>`
  : `<span class="unit-token" data-unit-id="${unit.id}" title="${unit.name}">${html}</span>`;
function layout(container, state) {
  const width = container.clientWidth, height = container.clientHeight;
  if (!state.units.length) { container.replaceChildren(); state.mode = ''; return; }
  // Only the native frame gives the container a size of its own; elsewhere it grows with its icons.
  if (!container.closest('#bottom-panel[data-frame=native]')) {
    if (state.mode !== 'plain') { state.mode = 'plain'; container.removeAttribute('data-layout'); container.innerHTML = state.units.map(unit => token(unit, state.icon(unit.kind, unit.tint))).join(''); }
    return;
  }
  if (width <= 0 || height <= 0) return;
  let size = tokenSize(width, height, state.units.length), mode = 'units';
  if (!size) {
    mode = 'groups'; state.groups ??= groupUnits(state.units);
    size = tokenSize(width, height, state.groups.length) || tokenSize(width, height, state.groups.length, SMALLEST - 1, TINIEST) || TINIEST;
  }
  container.style.setProperty('--token', `${size}px`);
  container.dataset.layout = mode;
  if (state.mode === mode) return;
  state.mode = mode;
  const html = new Map(), icon = unit => {
    const key = `${unit.kind}|${unit.tint ?? ''}`;
    if (!html.has(key)) html.set(key, state.icon(unit.kind, unit.tint));
    return html.get(key);
  };
  container.innerHTML = (mode === 'groups' ? state.groups.map(unit => token(unit, icon(unit), unit.count)) : state.units.map(unit => token(unit, icon(unit)))).join('');
}
export function renderSelectionIcons(container, units, icon) {
  let state = states.get(container);
  if (!state) {
    state = { units: [], groups: null, mode: '', icon };
    states.set(container, state);
    new ResizeObserver(() => layout(container, state)).observe(container);
    // A group icon selects the whole type; shift removes the whole type.
    container.addEventListener('click', event => {
      const group = event.target instanceof Element && event.target.closest('.unit-group');
      if (!group || event.ctrlKey) return;
      event.stopImmediatePropagation(); event.preventDefault();
      group.dispatchEvent(new MouseEvent('click', { bubbles: true, ctrlKey: true, shiftKey: event.shiftKey }));
    }, true);
  }
  state.units = units; state.groups = null; state.mode = ''; state.icon = icon;
  layout(container, state);
}
// Keeps a run of same-size tokens (garrison, training queue) inside its
// container as tokens come and go and the container changes size. The
// container's size comes from CSS; `--token` is the largest size that fits.
export function autoFit(container, { high = 30, low = 22, gap = 2 } = {}) {
  const fit = () => {
    const width = container.clientWidth, height = container.clientHeight, count = container.children.length;
    if (width <= 0 || height <= 0 || !count) return;
    const size = tokenSize(width, height, count, high, low, gap) || tokenSize(width, height, count, low - 1, TINIEST, gap) || TINIEST;
    container.style.setProperty('--token', `${size}px`);
  };
  new ResizeObserver(fit).observe(container);
  new MutationObserver(fit).observe(container, { childList: true });
  fit();
}
