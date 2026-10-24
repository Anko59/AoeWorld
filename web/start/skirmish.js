// A skirmish with AI opponents (web/start/README.md). Once the map job has
// completed, POST /skirmish builds the match and starts the AI players; the
// loading screen shows each server phase with its measured counts until every
// AI holds its seat. Cancel and Quit to Main Menu stop the AI players.
const POLL_MS = 500;

export function initSkirmish({ readJson, controllerHeaders, onStatus }) {
  let running = false;
  let cancelled = false;
  // The match swap resets the page's token until the game reconnects; the
  // server also accepts the one that started the skirmish.
  let starter = null;
  const headers = () => { try { return controllerHeaders(); } catch (error) { if (starter) return starter; throw error; } };
  const status = async () => readJson(await fetch('/skirmish'));
  const post = async (path, body) => readJson(await fetch(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...headers() },
    body: body === undefined ? undefined : JSON.stringify(body),
  }));
  // Resolves to an activation once every AI player is in, null if cancelled.
  async function run(hash, opponents) {
    running = true;
    cancelled = false;
    starter = controllerHeaders();
    try {
      const started = await post('/skirmish', { map_hash: hash, opponents });
      for (let value = started; ; value = await status()) {
        if (value.id !== started.id) throw new Error('Another game was started on this server.');
        onStatus?.(value);
        if (value.phase === 'ready') return { content_hash: hash, start_available: true };
        if (cancelled || value.phase === 'cancelled') return null;
        if (value.phase === 'failed' || value.phase === 'stopped') {
          throw new Error(value.error || 'The match could not start.');
        }
        await new Promise(resolve => setTimeout(resolve, POLL_MS));
      }
    } finally {
      running = false;
    }
  }
  return {
    run,
    running: () => running,
    status,
    async cancel() {
      cancelled = true;
      await post('/skirmish/stop');
    },
    // Quit to Main Menu: the AI players leave with the person.
    stop() { return post('/skirmish/stop').catch(() => null); },
  };
}
