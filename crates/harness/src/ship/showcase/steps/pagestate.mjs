// What the filmed page says about itself, for a take that is stuck: which
// start state it is in, what the loading screen shows, whether the game
// connection or the controller role is missing. Read by steps.mjs when a wait
// times out (the message names the state) and while it waits (a page that
// has already failed for good fails the step at once instead of at the limit).

// One read of the page. Missing elements read as null.
export async function snapshot(page) {
  return page.evaluate(() => {
    const text = (id) => document.getElementById(id)?.textContent?.trim() || null;
    const shown = (id) => {
      const node = document.getElementById(id);
      return node !== null && !node.hidden && getComputedStyle(node).display !== "none";
    };
    const scene = document.getElementById("scene");
    const minimap = document.getElementById("minimap");
    const attr = (node, name) => node?.getAttribute(name) ?? null;
    let token = false;
    try { token = /^[0-9a-f]{48}$/i.test(sessionStorage.getItem("aoeworld.resume-token") || ""); } catch { /* none */ }
    return {
      start_state: document.body?.dataset.startState ?? null,
      connection: text("connection"),
      observing: shown("observer-label"),
      controller_token: token,
      unsupported: text("unsupported"),
      loading_title: shown("loading-screen") ? text("loading-title") : null,
      loading_phase: shown("loading-screen") ? text("loading-phase") : null,
      loading_error: shown("loading-error") ? text("loading-error") : null,
      setup_status: text("setup-status"),
      chunks_pending: attr(scene, "data-chunks-pending"),
      chunks_failed: attr(scene, "data-chunks-failed"),
      frame: attr(scene, "data-frame"),
      raster: attr(minimap, "data-raster"),
      map_hash: attr(minimap, "data-map-hash"),
    };
  });
}

// A page that can no longer reach the state a step waits for: the client
// failed to start, or the loading screen shows its error. Returns the reason,
// or null while the page may still get there.
export function fatal(state) {
  if (state.unsupported) return `the client could not start: ${state.unsupported}`;
  if (state.loading_error) return `the loading screen shows an error: ${state.loading_error}`;
  if (state.connection === "Unavailable") return "the game connection reads Unavailable";
  return null;
}

// The server keeps a lost controller's lease for 30 s before it promotes a
// waiting observer (LEASE_RESERVATION, crates/server/src/gameplay_transport.rs).
// A page still observing after that has another live controller (a tab left
// from an earlier take) that this take cannot displace.
export const LEASE_WAIT_MS = 45_000;

// The reason to give up, or null: the page has watched instead of played
// since `since` (a timestamp, or null when it is not observing).
export function lostLease(since, now) {
  if (since === null || now - since < LEASE_WAIT_MS) return null;
  return `the page has only observed for ${Math.round((now - since) / 1000)} s: another tab or session holds the game's controller lease (the server frees a lost one after 30 s)`;
}

// While the start sequence opens a map the client may meet a subscription the
// server rejects (its region was computed for the previous world's size): its
// connection label then reads `server error: …` and, unlike every other
// status, only a new Welcome clears it, so the sequence (which waits for
// `connected`) never ends. Pressing the game menu's own Reconnect cures it.
// Reconnecting is tried after the label has read an error for this long, at
// most RECONNECTS times a step.
export const ERROR_SETTLE_MS = 4000;
export const RECONNECTS = 3;

// Should the harness press Reconnect now? `since` is when the label began
// to read a server error (null when it does not).
export function wantsReconnect(state, since, now) {
  return state.start_state === "loading" && since !== null && now - since >= ERROR_SETTLE_MS;
}

export const serverError = (state) => typeof state.connection === "string" && state.connection.startsWith("server error");

// The state as one line of `key=value` pairs, without the empty ones.
export function describe(state) {
  return Object.entries(state)
    .filter(([, value]) => value !== null && value !== false && value !== "")
    .map(([key, value]) => `${key}=${JSON.stringify(value)}`)
    .join(" ");
}

// The part of the state that marks progress through the start sequence (not
// the frame counter, which changes every frame), as a line that changes only
// when the page moves on.
export function transition(state) {
  const { start_state, connection, loading_title, loading_phase, chunks_pending, raster, observing, controller_token } = state;
  return describe({ start_state, connection, loading_title, loading_phase, chunks_pending, raster, observing, controller_token });
}

// The last console errors and failed requests of the page, newest last.
export function watch(page, limit = 8) {
  const seen = [];
  const note = (line) => { seen.push(String(line).slice(0, 300)); if (seen.length > limit) seen.shift(); };
  page.on("console", (message) => { if (message.type() === "error") note(`console: ${message.text()}`); });
  page.on("pageerror", (error) => note(`pageerror: ${error.message}`));
  page.on("requestfailed", (request) => note(`requestfailed: ${request.url()} ${request.failure()?.errorText ?? ""}`));
  return seen;
}
