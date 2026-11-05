// Cheap kingdom history: one ring of 240 fixed-width samples (a fixed array
// per series, no per-frame allocation). A sample is taken at most every
// STEP_MS of wall time from the cockpit's HUD-cadence reports.
const CAP = 240, STEP_MS = 5000;
export const resourceColours = { wood: '#c28a4b', food: '#d65c6a', gold: '#f0c63e', stone: '#aeb4c0' };
export const armyClasses = [
  ['Infantry', '#d9c39b', ['Militia', 'ManAtArms', 'Spearman', 'LongSwordsman', 'Pikeman', 'TwoHandedSwordsman', 'Champion', 'Halberdier']],
  ['Ranged', '#79cf91', ['Archer', 'Skirmisher', 'Crossbowman', 'EliteSkirmisher', 'Arbalester']],
  ['Cavalry', '#7fb4ee', ['Scout', 'Knight', 'LightCavalry', 'Cavalier', 'Paladin', 'Hussar']],
  ['Siege', '#e8a071', ['Mangonel', 'BatteringRam', 'CappedRam', 'SiegeRam', 'Onager']],
  ['Other', '#c9a1e6', []],
];
export function armyClassOf(kind) {
  const found = armyClasses.findIndex(([, , kinds]) => kinds.includes(kind));
  return found < 0 ? armyClasses.length - 1 : found;
}
export function createHistory() {
  const names = ['wood', 'food', 'gold', 'stone'];
  const series = {};
  for (const name of names) { series[`stock-${name}`] = new Float64Array(CAP); series[`rate-${name}`] = new Float64Array(CAP); }
  for (const key of ['pop', 'cap', 'army', ...armyClasses.map((_, i) => `class-${i}`)]) series[key] = new Float64Array(CAP);
  let length = 0, last = -Infinity, revision = 0;
  const push = (key, value) => {
    const data = series[key];
    if (length < CAP) data[length] = value; else { data.copyWithin(0, 1); data[CAP - 1] = value; }
  };
  return {
    revision: () => revision,
    length: () => length,
    series,
    // values = cockpit report, classes = army counts per class. NaN gaps stay NaN.
    sample(values, classes, now = performance.now()) {
      if (now - last < STEP_MS || !Number.isFinite(values[0])) return false;
      last = now;
      names.forEach((name, i) => { push(`stock-${name}`, values[i]); push(`rate-${name}`, values[6 + i]); });
      push('pop', values[4]); push('cap', values[5]);
      let total = 0;
      classes.forEach((count, i) => { push(`class-${i}`, count); total += count; });
      push('army', total);
      if (length < CAP) length++;
      revision++;
      return true;
    },
    reset() { length = 0; last = -Infinity; revision++; for (const data of Object.values(series)) data.fill(0); },
  };
}

const compact = value => value >= 1e6 ? `${+(value / 1e6).toFixed(1)}M` : value >= 1e3 ? `${+(value / 1e3).toFixed(1)}k` : String(Math.round(value));
// Draw lines or a stacked area for `keys` of a history. The vertical scale is
// the largest plotted value, labelled once; the horizontal span is the real
// elapsed time (STEP_MS per sample).
export function drawChart(canvas, history, keys, colours, { stacked = false, label = '', normalize = false } = {}) {
  const ratio = Math.min(2, window.devicePixelRatio || 1);
  const width = canvas.clientWidth || 300, height = canvas.clientHeight || 90;
  if (canvas.width !== Math.round(width * ratio)) canvas.width = Math.round(width * ratio);
  if (canvas.height !== Math.round(height * ratio)) canvas.height = Math.round(height * ratio);
  const context = canvas.getContext('2d');
  if (!context) return;
  context.setTransform(ratio, 0, 0, ratio, 0, 0); context.clearRect(0, 0, width, height);
  const left = 34, right = 6, top = 8, bottom = 14, w = width - left - right, h = height - top - bottom;
  const n = history.length();
  const data = keys.map(key => history.series[key]);
  let max = 1;
  for (let i = 0; i < n; i++) {
    let sum = 0;
    for (const d of data) { if (stacked) sum += d[i]; else max = Math.max(max, d[i]); }
    if (stacked) max = Math.max(max, sum);
  }
  max = niceMax(max);
  // normalize: every line is scaled to its own peak so small stocks stay readable.
  const peaks = data.map(d => { let m = 1; for (let i = 0; i < n; i++) m = Math.max(m, d[i]); return m; });
  context.font = '10px Georgia,serif'; context.textBaseline = 'middle';
  context.strokeStyle = '#3a2d1a40'; context.fillStyle = '#4a3a22'; context.lineWidth = 1;
  for (let g = 0; g <= 2; g++) {
    const y = top + h - h * g / 2;
    context.beginPath(); context.moveTo(left, y + .5); context.lineTo(left + w, y + .5); context.stroke();
    context.textAlign = 'right'; context.fillText(normalize ? `${g * 50}%` : compact(max * g / 2), left - 4, y);
  }
  context.textAlign = 'left'; context.textBaseline = 'alphabetic';
  const minutes = n * STEP_MS / 60000;
  context.fillText(minutes >= 1 ? `−${Math.round(minutes)} min` : `−${Math.round(n * STEP_MS / 1000)} s`, left, height - 2);
  context.textAlign = 'right'; context.fillText('now', left + w, height - 2);
  if (label) { context.textAlign = 'left'; context.fillText(label, left + 4, top + 8); }
  if (n < 2) { context.textAlign = 'center'; context.fillText('collecting…', left + w / 2, top + h / 2); return; }
  const x = i => left + w * i / (n - 1), y = v => top + h - h * Math.min(1, v / max);
  const floor = new Float64Array(n);
  data.forEach((d, k) => {
    context.beginPath();
    if (stacked) {
      for (let i = 0; i < n; i++) { const v = floor[i] + d[i]; i ? context.lineTo(x(i), y(v)) : context.moveTo(x(i), y(v)); }
      for (let i = n - 1; i >= 0; i--) context.lineTo(x(i), y(floor[i]));
      context.closePath(); context.fillStyle = colours[k] + 'cc'; context.fill();
      for (let i = 0; i < n; i++) floor[i] += d[i];
    } else {
      const at = i => normalize ? y(d[i] / peaks[k] * max) : y(d[i]);
      for (let i = 0; i < n; i++) i ? context.lineTo(x(i), at(i)) : context.moveTo(x(i), at(i));
      context.strokeStyle = colours[k]; context.lineWidth = 2; context.lineJoin = 'round'; context.stroke();
    }
  });
}
function niceMax(value) {
  const base = 10 ** Math.floor(Math.log10(Math.max(1, value)));
  for (const step of [1, 2, 2.5, 5, 10]) if (value <= base * step) return base * step;
  return value;
}
