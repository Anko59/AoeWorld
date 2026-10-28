import { initPerformance } from '/dev/performance.js';

// These presentation preferences are independent of the authoritative world.
export function editable(target) {
  return target instanceof Element && !!target.closest('input,textarea,select,[contenteditable]');
}

export function initDevTools() {
  const button = document.getElementById('carry-indicators');
  const loads = document.getElementById('carried-loads');
  const key = 'aoeworld.carryIndicators';
  let visible = true;
  try { visible = localStorage.getItem(key) !== 'false'; } catch { /* Use the in-memory preference. */ }
  function show(value) {
    visible = value;
    loads.hidden = !value;
    button.setAttribute('aria-pressed', String(value));
  }
  function toggle() {
    show(!visible);
    try { localStorage.setItem(key, String(visible)); } catch { /* Still usable without storage. */ }
  }
  show(visible);
  button.addEventListener('click', toggle);
  const performance = initPerformance();
  document.addEventListener('keydown', event => {
    if (event.repeat || event.ctrlKey || event.metaKey || event.altKey || editable(event.target)
        || document.querySelector('dialog[open]')) return;
    if (event.key.toLowerCase() === 'l') {
      event.preventDefault();
      toggle();
    } else if (event.key === 'F3') {
      event.preventDefault();
      performance.enable(!performance.enabled);
    }
  });
}
