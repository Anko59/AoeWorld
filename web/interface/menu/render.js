import { bindResearchTooltip, updateResearchTooltip } from '/interface/dom/research-tooltip.js';
import { set, attribute } from '/interface/dom/update.js';
import { artReady, entityIcon, icon, playerTint } from '/interface/ui-art.js';
import { renderSelectionIcons } from '/interface/selection-grid.js';
import { gridKeys } from './commands.js';

export const byId = id => document.getElementById(id);
export const show = (id, visible) => set(byId(id), 'hidden', !visible);
export const text = (id, value) => set(byId(id), 'textContent', value);
export const drawIcon = ([group, ...ident]) => group === 'entity' ? entityIcon(...ident) : icon(group, ...ident);

export function mountPanel(id, content = '') {
  const panel = Object.assign(document.createElement('div'), { id, hidden: true, innerHTML: content });
  (byId('selection-content') ?? document.body).append(panel);
  return panel;
}

const renderedMarkup = new WeakMap();
export function renderFields(fields) {
  for (const [id, properties] of Object.entries(fields)) {
    const node = byId(id);
    for (const [name, value] of Object.entries(properties)) {
      if (name === 'innerHTML') {
        if (renderedMarkup.get(node) === value) continue;
        renderedMarkup.set(node, value);
        node.innerHTML = value;
      } else if (name.startsWith('aria-') || name.startsWith('data-')) {
        attribute(node, name, value);
      } else {
        set(node, name, value);
      }
    }
  }
}

export function selectionCard() {
  let tokenKey = '', portraitKey = '';
  const portrait = byId('selection-portrait');
  const tokens = byId('selected-icons');
  return (card, selected) => {
    const { type, kind, count, first } = card;
    const health = count === 1 && card.hp != null;
    for (const key of ['name', 'owner', 'task', 'load']) text('selection-' + key, card[key] ?? '');
    renderFields({
      selection: { 'data-count': count, 'data-kind': kind ?? '', 'data-owned': Boolean(card.owned), 'data-type': type },
      'selection-empty': { hidden: Boolean(count) },
      'selection-content': { hidden: !count },
      'selection-health': health ? { hidden: false, max: Math.max(1, card.maxHp), value: card.hp } : { hidden: true },
      'unit-hp': { hidden: !health || card.building, textContent: health ? card.hp + ' / ' + card.maxHp + ' HP' : '' },
      'unit-combat': { hidden: !card.combat, textContent: card.combat },
    });
    const art = card.portrait ? ['units', ...card.portrait]
      : first && ['entity', first.kind, playerTint(first.player, first.owned, first.colour)];
    const key = art?.join('|') ?? '';
    if (key !== portraitKey) {
      portraitKey = key;
      portrait.innerHTML = art ? drawIcon(art) : '';
    }
    const groupKey = count > 1 ? selected.map(({ id, kind, colour }) => [id, kind, colour].join(':')).join(',') : '';
    if (groupKey === tokenKey) return;
    tokenKey = groupKey;
    renderSelectionIcons(tokens, count <= 1 ? [] : selected.map(actor => ({
      id: actor.id, kind: actor.kind, name: actor.name,
      tint: playerTint(actor.player, actor.owned, actor.colour),
    })), entityIcon);
  };
}

const next = { id: 'more-commands', label: 'More commands', key: ']', icon: ['commands', 'move', 'move'], order: 'next' };
const previous = { id: 'previous-commands', label: 'Previous commands', key: '[', icon: ['commands', 'move', 'move'], order: 'previous' };

export function commandSlots(commands, page = 0) {
  const size = gridKeys.length;
  const last = commands.length <= size ? 0 : Math.ceil(commands.length / (size - 2)) - 1;
  page = Math.min(page, last);
  const list = last ? commands.slice(page * (size - 2), (page + 1) * (size - 2)) : commands;
  const slots = Array(size).fill(null);
  if (last) {
    if (page > 0) slots[size - 2] = previous;
    if (page < last) slots[size - 1] = next;
  }
  const unplaced = list.filter(command => {
    const slot = gridKeys.indexOf(command.key || ' ');
    if (slot < 0 || slots[slot]) return true;
    slots[slot] = command;
    return false;
  });
  const available = slots.map((command, slot) => command ? -1 : slot).filter(slot => slot >= 0);
  for (const command of unplaced) {
    const researchSlot = command.research ? available.findIndex(slot => slot >= 5) : -1;
    const slot = available.splice(Math.max(0, researchSlot), 1)[0];
    // Explicit shortcuts that collided get the displayed slot key. This can
    // happen when the same selection contains both a General and a Transport.
    const special = command.key.length > 1 || command.key === ']' || command.key === '[';
    slots[slot] = { ...command, key: special ? command.key : gridKeys[slot] };
  }
  return { slots, page };
}

export function commandButtons() {
  let artLoaded = false;
  artReady.then(() => { artLoaded = true; });
  const buttons = new Map();
  return entry => {
    let button = buttons.get(entry.id);
    if (!button) {
      button = Object.assign(document.createElement('button'), { type: 'button', id: entry.id });
      buttons.set(entry.id, button);
    }
    const hasNote = entry.note !== undefined;
    const signature = JSON.stringify([artLoaded, entry.icon, entry.label, entry.key, entry.badge, entry.cost, hasNote]);
    if (button.dataset.artKey !== signature) {
      button.innerHTML = (entry.icon ? drawIcon(entry.icon) : '') +
        '<span class="command-label"></span><kbd></kbd><span class="trade-side" hidden></span>' +
        (hasNote ? '<small class="command-note"></small>' : '');
      const offset = entry.icon ? 1 : 0;
      const label = button.children[offset], shortcut = button.children[offset + 1], badge = button.children[offset + 2];
      label.textContent = [entry.label, entry.cost].filter(Boolean).join(' · ');
      shortcut.textContent = { Delete: 'Del', Escape: 'Esc' }[entry.key] ?? entry.key ?? '';
      shortcut.hidden = entry.key === undefined;
      badge.textContent = entry.badge ?? '';
      badge.hidden = !entry.badge;
      button.dataset.artKey = signature;
    }
    if (hasNote) set(button.lastElementChild, 'textContent', entry.note);
    set(button, 'className', entry.className ?? '');
    set(button, 'disabled', Boolean(entry.disabled));
    const title = [entry.label, entry.cost, entry.reason, entry.key].filter(Boolean).join(' · ');
    attribute(button, 'aria-label', entry.name ?? entry.label);
    if (entry.tooltip) {
      // Research cards describe the technology in a popover; the title keeps the
      // cost and requirement text (hidden only while the popover is open).
      if (!button.dataset.tooltip) { button.dataset.tooltip = '1'; bindResearchTooltip(button); }
      updateResearchTooltip(button, [entry.label.replace(/^Research /, ''), entry.tooltip, entry.reason].filter(Boolean).join(' · '), entry.title ?? title);
    } else {
      set(button, 'title', entry.title ?? title);
      attribute(button, 'aria-description', [entry.cost, entry.reason].filter(Boolean).join(' · '));
    }
    if (entry.key !== undefined) attribute(button, 'aria-keyshortcuts', entry.key);
    if (entry.pressed !== undefined) attribute(button, 'aria-pressed', entry.pressed);
    if (entry.price !== undefined) attribute(button, 'data-price', entry.price);
    return button;
  };
}

export function commandGrid(dispatch) {
  const host = byId('command-grid');
  const unavailable = document.createElement('div');
  unavailable.hidden = true;
  host.after(unavailable);
  let entries = [], hiddenIds = '';
  const emptyCells = Array.from(gridKeys, () => {
    const empty = document.createElement('span');
    empty.className = 'empty-command';
    empty.setAttribute('aria-hidden', 'true');
    return empty;
  });
  host.replaceChildren(...emptyCells);
  const renderButton = commandButtons();
  host.addEventListener('click', event => {
    const button = event.target.closest('button');
    if (!button || button.disabled) return;
    const entry = entries.find(command => command?.id === button.id);
    if (entry) dispatch(entry.order, event.shiftKey);
  });
  return {
    update(commands, page) {
      const layout = commandSlots(commands.filter(command => !command.hidden), page);
      const moved = layout.slots.some((entry, index) => entry?.id !== entries[index]?.id);
      entries = layout.slots;
      const next = entries.map((entry, index) => entry ? renderButton(entry) : emptyCells[index]);
      // A locked or completed research keeps its disabled state off the grid.
      const hidden = commands.filter(command => command.hidden);
      const ids = hidden.map(command => command.id).join(',');
      if (ids !== hiddenIds) {
        hiddenIds = ids;
        unavailable.replaceChildren(...hidden.map(renderButton));
      }
      if (moved) {
        const focus = document.activeElement;
        host.replaceChildren(...next);
        if (focus instanceof HTMLElement && focus.isConnected && document.activeElement !== focus) focus.focus();
      }
      return layout.page;
    },
    key(event) {
      const entry = entries.find(command => command?.key.toUpperCase() === event.key.toUpperCase());
      if (!entry) return false;
      event.preventDefault();
      if (!entry.disabled) dispatch(entry.order, event.shiftKey);
      return true;
    },
  };
}
