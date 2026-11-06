// View of the monotonic progress model (web/start/progress-model.js): fills the
// bar every frame and writes the percentage with the remaining-time estimate.
import { createProgressModel, formatRemaining } from './progress-model.js';

export function createLoadingBar({ bar, estimate, phase }) {
  let model = null;
  let frame = 0;
  let lastText = 0;
  let lastAria = -1;
  let last = null;
  const fill = bar.firstElementChild;
  const storage = (() => { try { return localStorage; } catch { return { getItem: () => null, setItem() {} }; } })();

  function draw() {
    frame = 0;
    if (!model) return;
    const { fraction, etaMs } = model.sample();
    fill.style.width = `${(fraction * 100).toFixed(2)}%`;
    const percent = Math.floor(fraction * 100);
    const now = performance.now();
    if (now - lastText > 250) {
      lastText = now;
      estimate.textContent = fraction >= 1 ? '100%'
        : `${percent}% · ${formatRemaining(etaMs)} remaining`;
    }
    if (percent !== lastAria) { lastAria = percent; bar.setAttribute('aria-valuenow', String(percent)); }
    if (fraction < 1) frame = requestAnimationFrame(draw);
  }
  function stop() {
    cancelAnimationFrame(frame);
    frame = 0;
    if (model) last = { trace: model.trace, run: model.run, weights: model.weights() };
    model = null;
  }
  return {
    /** A new creation starts: one model for everything until the game opens. */
    begin(request) {
      stop();
      model = createProgressModel({ now: () => performance.now(), storage, request });
      lastAria = -1;
      for (const [name, value] of [['aria-valuemin', '0'], ['aria-valuemax', '100'], ['aria-valuenow', '0']]) bar.setAttribute(name, value);
      fill.style.width = '0%';
      frame = requestAnimationFrame(draw);
    },
    active: () => model !== null,
    /** A server report: `key` is a progress-model stage; counts are the server's own. */
    update(key, completed, total, label) {
      model?.update(key, completed, total);
      bar.setAttribute('aria-valuetext', label);
      phase.textContent = label;
    },
    finish() { model?.finish(); },
    stop,
    // Measurement hook: the last run's samples, stage durations and weights.
    report: () => (model ? { trace: model.trace, run: model.run, weights: model.weights() } : last),
  };
}
