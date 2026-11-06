// Saved games on the main menu: list what the server holds and resume one.
// Resuming starts the match through the skirmish machinery (the loading screen,
// the AI opponents) from a save instead of a fresh map.
const size = bytes => (bytes >= 1048576 ? `${(bytes / 1048576).toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1024))} KB`);
const clock = seconds => `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;

export function initSaves({ readJson, tokenHeaders, sequence, skirmish }) {
  const el = id => document.getElementById(id);
  // Capture and play entries skip the menus (sequence.js) and have no list.
  if (!el('load-game')) return;
  const list = el('saves-list');
  const status = el('saves-status');

  async function refresh() {
    status.textContent = 'Looking for saved games…';
    list.replaceChildren();
    let cards;
    try { cards = await readJson(await fetch('/saves')); } catch (error) { status.textContent = `Could not read saved games: ${error.message}`; return; }
    status.textContent = cards.length ? '' : 'No saved games yet. Save from the game menu while playing.';
    for (const card of cards) {
      const row = document.createElement('li');
      row.dataset.save = card.id;
      const text = document.createElement('span');
      const title = document.createElement('strong');
      title.textContent = card.name;
      const detail = document.createElement('small');
      detail.textContent = `${new Date(card.saved_at * 1000).toLocaleString()} · game time ${clock(card.game_seconds)} · ${card.opponents} AI · ${card.units.toLocaleString()} units · ${size(card.bytes)}`;
      text.append(title, detail);
      const load = document.createElement('button');
      load.type = 'button';
      load.className = 'gold-button';
      load.textContent = 'Resume';
      load.setAttribute('aria-label', `Resume ${card.name}`);
      load.addEventListener('click', () => resume(card));
      const remove = document.createElement('button');
      remove.type = 'button';
      remove.className = 'quiet-button';
      remove.textContent = 'Delete';
      remove.setAttribute('aria-label', `Delete ${card.name}`);
      remove.addEventListener('click', async () => {
        await fetch(`/saves/${card.id}`, { method: 'DELETE', headers: tokenHeaders() }).catch(() => null);
        refresh();
      });
      row.append(text, load, remove);
      list.append(row);
    }
  }

  function resume(card) {
    sequence.loadSaved(card, () => skirmish.run(card.map_hash, card.opponents, card.id));
  }

  el('load-game').addEventListener('click', () => { sequence.showLoad(); refresh(); });
  el('load-back').addEventListener('click', () => sequence.menu());
}
