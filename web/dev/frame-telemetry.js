// CPU wall spans include synchronous JS/WASM/browser API work, never GPU time.
// Keep the last 2048 completed frames, including stalls; no match-long history.
const stages = ['update', 'interpolation', 'culling', 'terrain', 'sprites', 'ui', 'fog'];
const capacity = 2048;
export function frameTelemetry(clock = () => performance.now()) {
  const samples = Array.from({ length: 9 }, () => new Float64Array(capacity));
  const pending = new Float64Array(7), starts = Array(7).fill(null);
  let previous = null, interval = 0, draws = 0, count = 0, cursor = 0;
  function mark(section) {
    if (section !== 0 && section < 7) return;
    const now = clock();
    if (section === 0) {
      interval = previous === null ? 0 : Math.max(0, now - previous);
      previous = now;
    } else if (section >= 7 && section <= 13) starts[section - 7] = now;
    else if (section >= 14 && section <= 20) {
      const index = section - 14;
      if (starts[index] !== null) pending[index] += Math.max(0, now - starts[index]);
      starts[index] = null;
    } else if (section === 21) {
      samples[0][cursor] = interval;
      samples[1][cursor] = draws;
      for (let i = 0; i < 7; i++) samples[i + 2][cursor] = pending[i];
      pending.fill(0); draws = 0;
      count = Math.min(capacity, count + 1);
      cursor = (cursor + 1) % capacity;
    }
  }
  function quantiles(values) {
    const sorted = values.slice(0, count).sort();
    return count ? [sorted[Math.floor((count - 1) * .5)], sorted[Math.ceil((count - 1) * .95)], sorted[count - 1]] : [0, 0, 0];
  }
  function summary() {
    return { frames: count, frame_ms: quantiles(samples[0]), draw_calls: quantiles(samples[1]),
      stages_ms: Object.fromEntries(stages.map((name, i) => [name, quantiles(samples[i + 2])])) };
  }
  function reset() { previous = null; count = cursor = draws = 0; pending.fill(0); starts.fill(null); }
  return { mark, summary, reset, clear() { count = cursor = 0; }, draw() { draws++; } };
}

export function initFrameTelemetry() {
  const telemetry = frameTelemetry();
  let sent = performance.now(), posting = false;
  // Contour observations: cumulative frame counts since page load plus the last frame.
  let contour = { effect_frames: 0, plain_frames: 0, eligible_instances: 0, occluder_instances: 0,
    depth_span: 0, context_lost: 0, context_restored: 0 };
  const state = { latest: null, mark, contour(effect_frames, plain_frames, eligible_instances,
    occluder_instances, depth_span, context_lost, context_restored) {
    contour = { effect_frames, plain_frames, eligible_instances, occluder_instances,
      depth_span, context_lost, context_restored };
  } };
  function mark(section) {
    if (document.hidden) { telemetry.reset(); sent = performance.now(); return; }
    telemetry.mark(section);
    if (section !== 21 || performance.now() - sent < 5000) return;
    sent = performance.now();
    const report = { ...telemetry.summary(), renderer: document.getElementById('playground')?.dataset.renderer, contour };
    state.latest = report;
    telemetry.clear();
    // Keep cadence continuous across summary windows; reset only the history.
    let token;
    try { token = localStorage.getItem('aoeworld.resume-token'); } catch { return; }
    if (!token || !report.frames || posting) return;
    posting = true;
    fetch('/client/perf', { method: 'POST', signal: globalThis.AbortSignal?.timeout?.(4000), headers: { 'content-type': 'application/json',
      'x-aoeworld-controller-token': token }, body: JSON.stringify(report) })
      .catch(() => {}).finally(() => { posting = false; });
  }
  const restore = observeDraws(() => telemetry.draw());
  window.addEventListener('pagehide', event => { if (!event.persisted) restore(); });
  return state;
}

function observeDraws(sample) {
  const restore = [];
  for (const [type, methods] of [
    [globalThis.WebGL2RenderingContext, ['drawArrays', 'drawElements', 'drawArraysInstanced', 'drawElementsInstanced']],
    [globalThis.GPURenderPassEncoder, ['draw', 'drawIndexed', 'drawIndirect', 'drawIndexedIndirect']],
    [globalThis.CanvasRenderingContext2D, ['drawImage', 'fillRect', 'fill', 'stroke', 'putImageData', 'fillText']],
  ]) {
    if (!type) continue;
    for (const name of methods) {
      const prototype = type.prototype, descriptor = Object.getOwnPropertyDescriptor(prototype, name);
      if (typeof descriptor?.value !== 'function') continue;
      const original = descriptor.value;
      function measured(...args) {
        if (!this.canvas || (this.canvas.id === 'scene' || this.canvas.id === 'strategic-scene')) sample();
        return Reflect.apply(original, this, args);
      }
      try {
        Object.defineProperty(prototype, name, { ...descriptor, value: measured });
        restore.push(() => { if (prototype[name] === measured) Object.defineProperty(prototype, name, descriptor); });
      } catch { /* Native API instrumentation can be unavailable. */ }
    }
  }
  return () => { for (const undo of restore) { try { undo(); } catch { /* A native API may have become locked. */ } } };
}
