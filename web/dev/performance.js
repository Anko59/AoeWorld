import { gpuTiming, observeUploads } from '/dev/gpu-timing.js';

// Samples describe the preceding real game frame. No history grows with match
// duration, and disabled markers perform no clock reads or GPU work.
export function initPerformance() {
  const overlay = document.getElementById('performance-overlay');
  const names = ['decode', 'draw', 'ui'];
  const zero = () => ({ decode: 0, upload: 0, draw: 0, ui: 0 });
  const state = {
    enabled: false, frameMs: 0, cpuMs: zero(), gpuMs: null,
    gpuStatus: 'disabled', frames: 0, totalsMs: zero(),
    enable, mark,
  };
  let previous = 0, updated = 0, pending = zero(), unobserve = null;
  const starts = [null, null, null];
  const uploadsAtStart = [0, 0, 0];
  const gpu = gpuTiming((value, status) => { state.gpuMs = value; state.gpuStatus = status; });
  function enable(value) {
    if (state.enabled === value) return;
    state.enabled = value;
    overlay.hidden = !value;
    previous = updated = 0;
    pending = zero();
    starts.fill(null);
    if (value) {
      unobserve = observeUploads(ms => { pending.upload += ms; state.totalsMs.upload += ms; },
        () => { if (starts[1] !== null) gpu.begin(); });
      overlay.textContent = 'Frame: waiting\nCPU decode / upload / draw / UI: waiting\nGPU: waiting';
    } else {
      unobserve?.();
      unobserve = null;
      gpu.reset();
    }
  }
  function mark(section) {
    if (!state.enabled) return;
    const now = performance.now();
    if (section === 0) {
      state.frameMs = previous ? Math.max(0, now - previous) : 0;
      previous = now;
      state.cpuMs = pending;
      pending = zero();
      state.frames++;
      if (now - updated >= 250) {
        const cpu = state.cpuMs;
        overlay.textContent = `Frame: ${state.frameMs.toFixed(2)} ms\nLOD: ${document.getElementById('scene')?.dataset.lod ?? 'sprites'}\nCPU decode: ${cpu.decode.toFixed(2)} ms\nCPU upload: ${cpu.upload.toFixed(2)} ms\nCPU draw: ${cpu.draw.toFixed(2)} ms\nCPU UI: ${cpu.ui.toFixed(2)} ms\nGPU: ${state.gpuMs === null ? state.gpuStatus : state.gpuMs.toFixed(2) + ' ms'}\nF3: hide`;
        updated = now;
      }
    } else if (section <= 3) {
      starts[section - 1] = now;
      uploadsAtStart[section - 1] = pending.upload;
      if (section === 2) gpu.prepare();
    } else {
      const index = section - 4;
      if (starts[index] === null) return;
      const elapsed = Math.max(0, now - starts[index] - (pending.upload - uploadsAtStart[index]));
      pending[names[index]] += elapsed;
      state.totalsMs[names[index]] += elapsed;
      starts[index] = null;
      if (index === 1) gpu.end();
    }
  }
  window.aoePerformance = state;
  enable(new URLSearchParams(location.search).get('perf') === '1');
  return state;
}
