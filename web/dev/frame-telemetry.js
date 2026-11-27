// The diagnostic stage ring retains the latest frames. Cadence buckets count
// every interval in the minute, including intervals overwritten in the ring.
const stages = ['update', 'interpolation', 'culling', 'terrain', 'sprites', 'ui', 'fog'];
const capacity = 2048;
const cadenceCapacity = 32_768;
const minute = 60_000;
const overflow = 137;
const maxQueuedReports = 8;
const retryDelay = 5_000;

function cadence() {
  return { bins: new Uint32Array(overflow + 1), frames: 0, max: 0 };
}

function add(cadence, ms) {
  const bin = ms <= 100 ? Math.ceil(ms) : 100 + Math.ceil((ms - 100) / 25);
  cadence.bins[Math.min(bin, overflow)]++;
  cadence.frames++;
  cadence.max = Math.max(cadence.max, ms);
}

function describe(cadence) {
  const histogram = [];
  let over50 = 0, over100 = 0;
  for (let i = 0; i < cadence.bins.length; i++) {
    const count = cadence.bins[i];
    if (!count) continue;
    histogram.push([i, count]);
    if (i > 50) over50 += count;
    if (i > 100) over100 += count;
  }
  return { frames: cadence.frames, max_ms: cadence.max, over_50_ms: over50,
    over_100_ms: over100, histogram };
}

function percentile(still, pan, percent, max) {
  let rank = Math.ceil((still.frames + pan.frames) * percent / 100);
  if (!rank) return 0;
  for (let i = 0; i < overflow; i++) {
    rank -= still.bins[i] + pan.bins[i];
    if (rank <= 0) return Math.min(max, i <= 100 ? i : 100 + (i - 100) * 25);
  }
  return max;
}

export function frameTelemetry(clock = () => performance.now()) {
  const samples = Array.from({ length: 8 }, () => new Float64Array(capacity));
  const intervals = new Float64Array(cadenceCapacity);
  const pending = new Float64Array(7), starts = Array(7).fill(null);
  const still = cadence(), pan = cadence();
  let previous = null, began = null, interval = null, draws = 0, count = 0, cursor = 0;
  let lastX, lastY, panning = false;
  function mark(section) {
    if (section !== 0 && section < 7) return;
    if (section === 21) {
      if (interval !== null) {
        intervals[(still.frames + pan.frames) % cadenceCapacity] = interval;
        add(panning ? pan : still, interval);
      }
      samples[0][cursor] = draws;
      for (let i = 0; i < 7; i++) samples[i + 1][cursor] = pending[i];
      pending.fill(0); draws = 0;
      count = Math.min(capacity, count + 1);
      cursor = (cursor + 1) % capacity;
      return;
    }
    const now = clock();
    if (section === 0) {
      began ??= now;
      interval = previous === null ? null : Math.max(0, now - previous);
      previous = now;
    } else if (section >= 7 && section <= 13) starts[section - 7] = now;
    else if (section >= 14 && section <= 20) {
      const index = section - 14;
      if (starts[index] !== null) pending[index] += Math.max(0, now - starts[index]);
      starts[index] = null;
    }
  }
  function view(x, y) {
    panning = lastX !== undefined && (x !== lastX || y !== lastY);
    lastX = x; lastY = y;
    return began !== null && previous - began >= minute;
  }
  function quantiles(values) {
    const sorted = values.slice(0, count).sort();
    return count ? [sorted[Math.floor((count - 1) * .5)], sorted[Math.ceil((count - 1) * .95)], sorted[count - 1]] : [0, 0, 0];
  }
  function summary() {
    const max = Math.max(still.max, pan.max);
    const frames = still.frames + pan.frames;
    const sorted = frames > 0 && frames <= cadenceCapacity ? intervals.slice(0, frames).sort() : null;
    const quantile = percent => sorted
      ? sorted[Math.ceil(frames * percent / 100) - 1] : percentile(still, pan, percent, max);
    return { frames, frame_ms: [quantile(50), quantile(99), max],
      still: describe(still), panning: describe(pan), draw_calls: quantiles(samples[0]),
      stages_ms: Object.fromEntries(stages.map((name, i) => [name, quantiles(samples[i + 1])])) };
  }
  function clear() {
    count = cursor = 0;
    began = previous;
    for (const group of [still, pan]) {
      group.bins.fill(0);
      group.frames = group.max = 0;
    }
  }
  function suspend() {
    previous = interval = null;
    lastX = lastY = undefined;
    panning = false;
    draws = 0; pending.fill(0); starts.fill(null);
  }
  return { mark, view, summary, suspend, clear, draw() { draws++; } };
}

export function initFrameTelemetry() {
  const telemetry = frameTelemetry();
  const source = crypto.getRandomValues(new Uint32Array(1))[0];
  let sequence = 0, outbox = null, visible = !document.hidden;
  const collecting = () => visible && outbox && outbox.reports.length < maxQueuedReports;
  document.addEventListener('visibilitychange', () => {
    visible = !document.hidden;
    telemetry.suspend();
  });

  async function send(box) {
    if (box.sending || box.timer !== null || !box.reports.length) return;
    box.sending = true;
    let retry;
    try {
      const response = await fetch('/client/perf', {
        method: 'POST', signal: globalThis.AbortSignal?.timeout?.(4000),
        headers: { 'content-type': 'application/json', 'x-aoeworld-controller-token': box.token },
        body: box.reports[0],
      });
      // 409 confirms that a previous attempt already recorded this sequence.
      retry = response.status === 429 || response.status >= 500;
    } catch {
      retry = true;
    }
    box.sending = false;
    if (outbox !== box) return;
    if (retry) {
      box.timer = setTimeout(() => {
        box.timer = null;
        void send(box);
      }, box.delay);
      box.delay = Math.min(minute, box.delay * 2);
      return;
    }
    box.reports.shift();
    box.delay = retryDelay;
    void send(box);
  }

  const state = { latest: null, session(next) {
    if ((outbox?.token ?? '') === next) return;
    if (outbox?.timer != null) clearTimeout(outbox.timer);
    outbox = next ? { token: next, reports: [], sending: false, timer: null, delay: retryDelay } : null;
    state.latest = null;
    telemetry.suspend();
    telemetry.clear();
  }, mark(section) {
    if (collecting()) telemetry.mark(section);
  }, view(x, y) {
    return !!collecting() && telemetry.view(x, y);
  }, publish(detailedUnits, effect_frames, plain_frames, eligible_instances,
    occluder_instances, depth_span, context_lost, context_restored) {
    // Use the monotonic clock across minutes and give each page its own suffix.
    sequence = Math.max(sequence + 1, Math.floor((performance.timeOrigin + performance.now()) * 1000));
    const report = { ...telemetry.summary(), sequence: [sequence, source],
      units_in_view: window.aoeNavigation?.strategic?.visibleUnits?.() ?? detailedUnits,
      renderer: document.getElementById('playground')?.dataset.renderer,
      contour: { effect_frames, plain_frames, eligible_instances, occluder_instances,
        depth_span, context_lost, context_restored } };
    state.latest = report;
    if (collecting() && report.frames) {
      outbox.reports.push(JSON.stringify(report));
      // Pause sampling during a long backlog so completed minutes stay queued.
      if (outbox.reports.length === maxQueuedReports) telemetry.suspend();
      void send(outbox);
    }
    telemetry.clear();
  } };
  const restore = observeDraws(() => { if (collecting()) telemetry.draw(); });
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
