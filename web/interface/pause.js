// Pause (docs: crates/server/src/gameplay_match/pause.rs). The server stops
// stepping the match; this only asks, shows the state, and follows it. Keys:
// P and Pause.
const byId = id => document.getElementById(id);
const POLL_MS = 3000;

const headers = () => {
  const token = localStorage.getItem('aoeworld.resume-token');
  if (!/^[0-9a-f]{48}$/i.test(token || '')) throw new Error('not connected');
  return { 'Content-Type': 'application/json', 'X-AoeWorld-Controller-Token': token };
};

export function initPause(menu) {
  const toggle = byId('pause-game');
  const banner = byId('game-paused');
  let paused = false;
  const show = value => {
    paused = value;
    banner.hidden = !value;
    toggle.textContent = value ? 'Resume game' : 'Pause game';
    toggle.setAttribute('aria-pressed', String(value));
    document.body.dataset.paused = String(value);
  };
  const set = async value => {
    try {
      const response = await fetch('/match/pause', { method: 'POST', headers: headers(), body: JSON.stringify({ paused: value }) });
      if (response.ok) show((await response.json()).paused);
    } catch (error) { /* offline or not the controller: keep the shown state */ }
  };
  toggle.addEventListener('click', async () => { await set(!paused); menu.close(); byId('scene').focus(); });
  byId('unpause-game').addEventListener('click', () => set(false));
  // Follow the server: a reload, another page, or a new match changes it.
  const follow = async () => {
    try {
      const response = await fetch('/skirmish');
      if (response.ok) { const value = await response.json(); if (value.paused !== paused) show(Boolean(value.paused)); }
    } catch (error) { /* retry on the next poll */ }
  };
  // P or Pause toggles in the game; F1-F6 are taken by the map layers and views.
  document.addEventListener('keydown', event => {
    if (event.key !== 'Pause' && event.key !== 'p') return;
    if (event.repeat || event.ctrlKey || event.altKey || event.metaKey || event.defaultPrevented) return;
    if (document.body.dataset.startState !== 'game' || document.querySelector('dialog[open]')) return;
    if (event.target instanceof Element && event.target.closest('input,textarea,select,[contenteditable]')) return;
    event.preventDefault();
    set(!paused);
  });
  follow();
  setInterval(follow, POLL_MS);
}
