// Save game (crates/server/src/gameplay_match/saves.rs): the game menu asks the
// server to write the running match to disk and reports what it measured.
const byId = id => document.getElementById(id);

const headers = () => {
  const token = localStorage.getItem('aoeworld.resume-token');
  if (!/^[0-9a-f]{48}$/i.test(token || '')) throw new Error('not connected');
  return { 'Content-Type': 'application/json', 'X-AoeWorld-Controller-Token': token };
};

const size = bytes => (bytes >= 1048576 ? `${(bytes / 1048576).toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1024))} KB`);

export function initSaveGame() {
  const form = byId('save-form');
  const name = byId('save-name');
  const status = byId('save-status');
  const button = byId('save-game');
  form.addEventListener('submit', async event => {
    event.preventDefault();
    const label = name.value.trim() || `Game ${new Date().toLocaleString()}`;
    button.disabled = true;
    status.textContent = 'Saving…';
    try {
      const response = await fetch('/saves', { method: 'POST', headers: headers(), body: JSON.stringify({ name: label }) });
      if (!response.ok) throw new Error(await response.text());
      const card = await response.json();
      status.textContent = `Saved “${card.name}” · ${card.units.toLocaleString()} units · ${size(card.bytes)} · ${(card.save_ms / 1000).toFixed(1)} s`;
      status.dataset.saved = card.id;
    } catch (error) {
      status.textContent = `Could not save: ${error.message}`;
      delete status.dataset.saved;
    } finally {
      button.disabled = false;
    }
  });
}
