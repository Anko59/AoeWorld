// Positions and cargo counts come from the observed authoritative ships.
let hint, layer;
const loads = new Map();
export function transportHint(board) {
  if (!hint) {
    hint = document.createElement('div');
    hint.id = 'transport-hint';
    hint.textContent = 'Board Transport Ship';
    document.body.append(hint);
  }
  hint.hidden = !board;
}
export function transportLoads(values) {
  if (!layer) {
    layer = document.createElement('div');
    layer.id = 'transport-loads';
    layer.setAttribute('aria-label', 'Transport passengers');
    document.body.append(layer);
  }
  const visible = new Set();
  for (let i = 0; i + 3 < values.length; i += 4) {
    const [id, x, y, cargo] = values.slice(i, i + 4);
    visible.add(id);
    let label = loads.get(id);
    if (!label) {
      label = document.createElement('span');
      label.className = 'transport-load';
      label.dataset.ship = String(id);
      layer.append(label);
      loads.set(id, label);
    }
    label.textContent = `${cargo}/5`;
    label.style.left = `${x}px`;
    label.style.top = `${y}px`;
  }
  for (const [id, label] of loads) if (!visible.has(id)) {
    label.remove();
    loads.delete(id);
  }
}
