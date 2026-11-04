// Camera bookmarks live for this world. Gameplay targets come only from the
// authorized army reports and actual damage events, never inferred threats.
export function initViews(jump) {
  const style = document.createElement('link');
  style.rel = 'stylesheet'; style.href = '/navigation/views.css'; document.head.append(style);
  const nav = document.createElement('nav');
  nav.id = 'strategic-views'; nav.hidden = true;
  nav.setAttribute('aria-label', 'Jump to view');
  nav.innerHTML = `<b>Jump to view</b><div class="view-targets">
    <button type="button" data-home>Main base H</button>
    <button type="button" data-alert disabled>Last alert Space</button>
    <button type="button" data-army disabled>Largest army End</button></div>
    <div class="view-slots">${[1, 2, 3, 4].map(n => `<span><button type="button" data-view="${n - 1}" disabled title="Recall · Shift+F${n}">View ${n}</button><button type="button" data-save="${n - 1}" title="Save position and zoom · Ctrl+F${n}">Save</button></span>`).join('')}</div>
    <small>Ctrl+F1–F4 save · Shift+F1–F4 recall</small>`;
  document.body.append(nav);
  const slots = [null, null, null, null];
  const recall = [...nav.querySelectorAll('[data-view]')];
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
    slots[index] = [...camera]; recall[index].disabled = false;
    recall[index].title = `Recall (${camera[0].toFixed(1)}, ${camera[1].toFixed(1)}) at ${(camera[2] * 100).toPrecision(3)}% · Shift+F${index + 1}`;
  }
  nav.querySelector('[data-home]').addEventListener('click', () => {
    document.getElementById('town-center')?.click();
    document.getElementById('scene')?.focus();
  });
  alertButton.addEventListener('click', () => focus(alert));
  armyButton.addEventListener('click', () => focus(army));
  recall.forEach((node, index) => node.addEventListener('click', () => focus(slots[index])));
  nav.querySelectorAll('[data-save]').forEach((node, index) => node.addEventListener('click', () => save(index)));
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
      slots.fill(null); recall.forEach(node => { node.disabled = true; });
      camera = army = alert = null; alertTick = -1;
      armyButton.disabled = alertButton.disabled = true;
    },
  };
}
