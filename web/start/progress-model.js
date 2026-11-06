// One monotonic loading bar for the whole of map creation (web/start/README.md).
//
// The stages below run in this order: the map job's own phases, the match
// build, then the terrain wait. Each has an expected duration, scaled by map
// size and opponents and calibrated by this machine's recent runs. The bar is
// elapsed / (elapsed + estimated remaining), interpolated every frame: it never
// goes backwards, never stops, and the remaining-time estimate converges as the
// run reveals its own speed. Pure: the clock and storage are injected.
export const STAGES = [
  // Measured 2026-11-06, Paris 480 km, 7 opponents, a busy 12-core machine
  // (load 15-20); unmeasured stages carry a stated guess.
  ['queued', 500, 0, 0],
  ['preparing_sources', 34000, 1, 0], // measured 34 s
  ['downloading_source', 20000, 1, 0], // guess: skipped when sources are cached
  ['sampling_overview', 40000, 1, 0], // measured 40 s (1,388 pages)
  ['sampling_water', 100000, 1, 0], // measured 102 s
  ['building_pyramids', 20000, 1, 0], // guess: not seen in this run
  ['publishing_package', 800, 0, 0], // measured 0.8 s
  ['verifying_package', 1500, 0, 0], // measured 1.4 s
  ['activating', 300, 0, 0],
  ['opening_map', 600, 1, 0], // measured 0.5 s
  ['placing_villages', 1000, 0, 1], // measured 1.0 s
  ['starting_ai', 1000, 0, 1], // skipped in the run: under 0.5 s
  ['connecting_ai', 700, 0, 1], // measured 0.7 s
  ['opening_terrain', 6000, 0, 0], // see README: the measured wait included a hidden pane
];
const REFERENCE = { size: 480, opponents: 7 };
const HISTORY_KEY = 'aoeworld.load-history';
const RUNS_KEPT = 6;
const CEILING = 0.995; // Only the real finish reaches 100%.
const SMOOTHING_MS = 450;
const ETA_BLEND_MS = 6000;
const PRIOR_SHARE = 0.08; // Of the expected total: shrinks the speed ratio toward 1 while little is seen.

const index = key => STAGES.findIndex(stage => stage[0] === key);
const clamp = (value, low, high) => Math.min(high, Math.max(low, value));
const median = values => {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted.length ? sorted[Math.floor(sorted.length / 2)] : 1;
};

/** Expected duration of one stage for a request, before calibration. */
export function expectedMs(key, { size, opponents }) {
  const stage = STAGES[index(key)];
  if (!stage) return 3000;
  const [, base, sizePower, opponentPower] = stage;
  const scaledSize = Math.max(size, 1) / REFERENCE.size;
  const scaledOpponents = Math.max(opponents, 1) / REFERENCE.opponents;
  return base * scaledSize ** (sizePower * 2) * scaledOpponents ** opponentPower;
}

export function loadHistory(storage) {
  try {
    const runs = JSON.parse(storage.getItem(HISTORY_KEY));
    return Array.isArray(runs) ? runs.filter(run => run && typeof run.stages === 'object').slice(-RUNS_KEPT) : [];
  } catch { return []; }
}

/** Machine speed per stage: median of recorded / default over recent runs. */
function calibration(history, key) {
  const ratios = history
    .filter(run => Number.isFinite(run.stages[key]) && run.stages[key] > 0)
    .map(run => run.stages[key] / expectedMs(key, run));
  return ratios.length ? clamp(median(ratios), 0.2, 8) : 1;
}

export function createProgressModel({ now, storage, request }) {
  const history = loadHistory(storage);
  const run = { size: request.size, opponents: request.opponents, signature: request.signature, stages: {} };
  // The same request last time completed its map stages at once: a stored map.
  const cached = history.some(entry => entry.signature === request.signature && entry.cached);
  const weight = STAGES.map(([key]) => expectedMs(key, request) * calibration(history, key));
  const skipped = STAGES.map(([key], at) => cached && at >= 1 && at <= index('verifying_package'));
  const actual = STAGES.map(() => null);
  const started = now();
  let current = 0;
  let stageStart = started;
  let counts = null; // { fraction, at, rate }
  let shown = 0;
  let eta = null;
  let last = started;
  let finished = false;
  let activatedAt = null;
  const trace = [];
  let traced = -1e9;

  function enter(key) {
    const next = index(key);
    if (next <= current || next < 0) return false; // A repeated or older phase never rewinds.
    const t = now();
    actual[current] = t - stageStart;
    for (let at = current + 1; at < next; at++) skipped[at] = true;
    skipped[next] = false;
    if (key === 'activating') activatedAt = t;
    current = next;
    stageStart = t;
    counts = null;
    return true;
  }
  function update(key, completed, total) {
    enter(key);
    if (index(key) !== current) return;
    if (!(Number.isSafeInteger(completed) && Number.isSafeInteger(total) && total > 0)) return;
    const fraction = clamp(completed / total, 0, 1);
    const t = now();
    if (!counts || fraction > counts.fraction) {
      const rate = counts && t > counts.at ? (fraction - counts.fraction) / (t - counts.at) : counts?.rate ?? 0;
      counts = { fraction, at: t, rate: counts ? 0.6 * rate + 0.4 * counts.rate : 0, total };
    }
  }
  function stageFraction(t) {
    const expected = weight[current];
    if (counts) {
      const ahead = counts.rate > 0 ? counts.rate * (t - counts.at) : 0;
      return clamp(counts.fraction + Math.min(ahead, 1 / counts.total), 0, CEILING);
    }
    const x = (t - stageStart) / expected;
    return x < 0.9 ? x : 0.9 + 0.09 * (1 - Math.exp(-(x - 0.9) / 0.5));
  }
  function estimate(t) {
    const elapsed = t - started;
    const p = stageFraction(t);
    const inStage = t - stageStart;
    const prior = PRIOR_SHARE * weight.reduce((sum, ms, at) => sum + (skipped[at] ? 0 : ms), 0);
    let seen = prior;
    let expectedSeen = prior;
    for (let at = 0; at < current; at++) {
      if (skipped[at] || actual[at] === null) continue;
      seen += actual[at];
      expectedSeen += weight[at];
    }
    if (inStage > weight[current]) { seen += inStage; expectedSeen += weight[current]; }
    const ratio = clamp(seen / expectedSeen, 0.15, 6);
    let remaining = weight[current] > 0 ? Math.max(weight[current] * (1 - p), 0.25 * weight[current] * (inStage > weight[current] ? 1 : 0)) * ratio : 0;
    if (counts && counts.rate > 0 && counts.fraction > 0.05) remaining = (1 - counts.fraction) / counts.rate;
    for (let at = current + 1; at < STAGES.length; at++) if (!skipped[at]) remaining += weight[at] * ratio;
    return { elapsed, remaining };
  }
  function sample() {
    const t = now();
    const dt = Math.max(0, t - last);
    last = t;
    if (finished) { shown = Math.min(1, shown + dt / 400); return { fraction: shown, etaMs: 0, stage: STAGES[current][0] }; }
    const { elapsed, remaining } = estimate(t);
    const target = clamp(elapsed / (elapsed + remaining), 0, CEILING);
    // Ease toward the target; when it dips below, creep asymptotically instead.
    const eased = shown + (target - shown) * (1 - Math.exp(-dt / SMOOTHING_MS));
    const creep = shown + (CEILING - shown) * Math.min(dt, 1000) / Math.max(remaining * 3, 20000);
    shown = Math.max(shown, Math.min(CEILING, Math.max(eased, creep)));
    eta = eta === null ? remaining : eta - dt + (remaining - (eta - dt)) * (1 - Math.exp(-dt / ETA_BLEND_MS));
    eta = Math.max(0, eta);
    if (t - traced >= 1000) { traced = t; trace.push({ at: Math.round(elapsed), shown: +shown.toFixed(4), eta: Math.round(eta), stage: STAGES[current][0] }); }
    return { fraction: shown, etaMs: eta, stage: STAGES[current][0] };
  }
  /** The game opened: remember how long each observed stage took. */
  function finish() {
    if (finished) return;
    const t = now();
    actual[current] = t - stageStart;
    finished = true;
    STAGES.forEach(([key], at) => { if (!skipped[at] && actual[at] !== null) run.stages[key] = Math.round(actual[at]); });
    // A stored map completes its job stages at once; the next run can expect that.
    run.cached = activatedAt !== null && activatedAt - started < 3000;
    run.total = Math.round(t - started);
    if (request.record === false) return; // A reload's wait measures page start-up, not creation.
    try { storage.setItem(HISTORY_KEY, JSON.stringify([...history, run].slice(-RUNS_KEPT))); } catch { /* Estimates fall back to defaults. */ }
  }
  return { update, sample, finish, trace, run, weights: () => STAGES.map(([key], at) => [key, Math.round(weight[at]), skipped[at]]) };
}

/** "about 1 min 20 s" in steps that do not flicker. */
export function formatRemaining(ms) {
  const seconds = Math.round(ms / 1000);
  if (seconds < 4) return 'a few moments';
  if (seconds < 60) return `about ${Math.max(5, Math.round(seconds / 5) * 5)} s`;
  const rounded = Math.round(seconds / 10) * 10;
  const minutes = Math.floor(rounded / 60);
  return rounded % 60 ? `about ${minutes} min ${rounded % 60} s` : `about ${minutes} min`;
}
