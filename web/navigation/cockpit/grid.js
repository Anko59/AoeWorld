import { entityIcon, icon } from '/interface/ui-art.js';

// One-click icon grid. Each cell shows original art, a one-line name and a
// state; there are no drop-downs. `cells` = [{ id, label, art, note }].
export function iconGrid(label, cells, pick) {
  const grid = document.createElement('div');
  grid.className = 'far-grid'; grid.setAttribute('role', 'group'); grid.setAttribute('aria-label', label);
  const buttons = new Map();
  for (const cell of cells) {
    const button = document.createElement('button');
    button.type = 'button'; button.className = 'far-cell'; button.dataset.id = String(cell.id);
    button.innerHTML = `${cell.art}<span class="far-cell-name">${cell.label}</span><small class="far-cell-note"></small>`;
    button.title = cell.label; button.setAttribute('aria-label', cell.label);
    button.addEventListener('click', () => { if (!button.disabled) pick(cell.id, button); });
    buttons.set(cell.id, button); grid.append(button);
  }
  return {
    grid,
    set(id, { enabled, note, title, selected, hidden } = {}) {
      const button = buttons.get(id);
      if (!button) return;
      if (enabled !== undefined) button.disabled = !enabled;
      if (hidden !== undefined) button.hidden = hidden;
      if (selected !== undefined) button.setAttribute('aria-pressed', String(selected));
      if (note !== undefined) { const node = button.querySelector('.far-cell-note'); if (node.textContent !== note) node.textContent = note; }
      if (title !== undefined) { button.title = title; button.setAttribute('aria-label', title); }
    },
    get: id => buttons.get(id),
  };
}
export const unitArt = kind => entityIcon(kind);
export const techArt = id => icon('technologies', id, 'technology');
