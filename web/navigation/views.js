import { icon, entityIcon } from '/interface/ui-art.js';
// Camera bookmarks live for this world. Gameplay targets come only from the
// authorized army reports and actual damage events, never inferred threats.
export function initViews(jump) {
  const nav = document.createElement('nav');
  nav.id = 'strategic-views'; nav.hidden = true;
  nav.setAttribute('aria-label', 'Jump to view');
  nav.innerHTML = `<button type="button" class="dock-btn" data-home aria-label="Main base H" title="Main base (H)">${entityIcon('TownCenter')}<kbd>H</kbd></button>
    <button type="button" class="dock-btn" data-alert disabled aria-label="Last alert Space" title="Last alert (Space)">${icon('commands', 'attack', 'soldier')}<kbd>␣</kbd></button>
    <button type="button" class="dock-btn" data-army disabled aria-label="Largest army End" title="Largest army (End)">${entityIcon('General')}<kbd>End</kbd></button>
    ${[1, 2, 3, 4].map(n => `<button type="button" class="dock-btn dock-slot" data-view="${n - 1}" aria-label="View ${n}" title="Empty · click to save this view (Ctrl+F${n}); recall with Shift+F${n}">${n}<kbd>F${n}</kbd></button>`).join('')}`;
  document.body.append(nav);
  const slots = [null, null, null, null];
  const recall = [...nav.querySelectorAll('[data-view]')];
  const saved = index => Boolean(slots[index]);
  const alertButton = nav.querySelector('[data-alert]');
  const armyButton = nav.querySelector('[data-army]');
  armyButton.title = 'Largest reported own General army';
  let camera = null, army = null, alert = null, alertTick = -1;
  function focus(view) {
    if (!view || !camera) return;
    jump(view.length === 3 ? [...view] : [...view, camera[2]]);
    document.getElementById('scene')?.focus();
  }
  function save(index) {
    if (!camera) return;
    slots[index] = [...camera]; recall[index].dataset.saved = 'true';
    recall[index].title = `Recall · click (shift/ctrl+click re-saves) (${camera[0].toFixed(1)}, ${camera[1].toFixed(1)}) at ${(camera[2] * 100).toPrecision(3)}% · Shift+F${index + 1}`;
  }
  nav.querySelector('[data-home]').addEventListener('click', () => {
    document.getElementById('town-center')?.click();
    document.getElementById('scene')?.focus();
  });
  alertButton.addEventListener('click', () => focus(alert));
  armyButton.addEventListener('click', () => focus(army));
  recall.forEach((node, index) => node.addEventListener('click', event => { if (!saved(index) || event.ctrlKey || event.shiftKey) save(index); else focus(slots[index]); }));
  document.addEventListener('keydown', event => {
    if (!camera || event.defaultPrevented || event.repeat || event.altKey || event.metaKey
      || document.querySelector('dialog[open]')
      || (event.target instanceof Element && event.target.closest('input,textarea,select,[contenteditable]'))) return;
    const index = ['F1', 'F2', 'F3', 'F4'].indexOf(event.key);
    if (index >= 0 && (event.ctrlKey || event.shiftKey)) {
      event.preventDefault();
      if (event.ctrlKey) save(index); else focus(slots[index]);
    } else if (!event.ctrlKey && !event.shiftKey && event.key === 'End') {
      event.preventDefault(); focus(army);
    } else if (!event.ctrlKey && !event.shiftKey && event.code === 'Space'
      && !(event.target instanceof Element && event.target.closest('button'))) {
      event.preventDefault(); focus(alert);
    }
  });
  return {
    show(value) { nav.hidden = !value; },
    present(state) {
      const next = [state[2], state[3], state[8]];
      if (next.every(Number.isFinite) && next[2] > 0) camera = next;
    },
    armies(values) {
      let largest = null;
      for (let at = 0; at + 14 < values.length && at < 256 * 15; at += 15) {
        const row = values.slice(at, at + 15);
        if (row[3] !== 1 || row[5] < 0 || ![row[0], row[1], row[2], row[5]].every(Number.isFinite)) continue;
        if (!largest || row[5] > largest[5] || (row[5] === largest[5] && row[0] < largest[0])) largest = row;
      }
      army = largest?.slice(1, 3) ?? null; armyButton.disabled = !army;
    },
    alert(tick, x, y) {
      if (![tick, x, y].every(Number.isFinite) || tick < 0 || tick <= alertTick) return;
      alertTick = tick; alert = [x, y]; alertButton.disabled = false;
      alertButton.title = 'Last observed damage to your units or buildings';
    },
    reset() {
      slots.fill(null); recall.forEach((node, i) => { delete node.dataset.saved; node.title = `Empty · click to save this view (Ctrl+F${i + 1}); recall with Shift+F${i + 1}`; });
      camera = army = alert = null; alertTick = -1;
      armyButton.disabled = alertButton.disabled = true;
    },
  };
}
