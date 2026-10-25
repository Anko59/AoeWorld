// One nonblocking timer query at most. A missing timer stays explicitly null;
// queue completion is not a GPU duration. WebGPU's device does not request the
// optional timestamp-query feature, so that backend reports unavailable.
export function gpuTiming(report) {
  let canvas = null, gl = null, extension = null, query = null, active = false;
  function clear() {
    const previous = query, running = active;
    query = null;
    active = false;
    if (!previous || !gl || gl.isContextLost()) return;
    try { if (running) gl.endQuery(extension.TIME_ELAPSED_EXT); } catch { /* Context may have failed. */ }
    try { gl.deleteQuery(previous); } catch { /* The failed context owns cleanup. */ }
  }
  function reset() {
    clear();
    canvas = gl = extension = null;
    report(null, 'disabled');
  }
  function prepare() {
    const next = document.getElementById('scene');
    if (next !== canvas) {
      clear();
      canvas = next;
      gl = null;
      extension = null;
      const backend = document.getElementById('playground')?.dataset.renderer;
      if (backend === 'webgl2') {
        gl = canvas.getContext('webgl2');
        extension = gl?.getExtension('EXT_disjoint_timer_query_webgl2');
      }
      report(null, backend === 'webgpu' ? 'WebGPU timestamp-query not enabled'
        : extension ? 'pending' : 'GPU timer unavailable');
    }
    if (gl?.isContextLost()) {
      clear();
      canvas = extension = null;
      report(null, 'GPU context lost');
      return false;
    }
    if (!extension || !gl) return false;
    if (gl.getParameter(extension.GPU_DISJOINT_EXT)) {
      clear();
      report(null, 'GPU clock disjoint');
      return false;
    }
    if (query) {
      if (!gl.getQueryParameter(query, gl.QUERY_RESULT_AVAILABLE)) return false;
      const nanoseconds = gl.getQueryParameter(query, gl.QUERY_RESULT);
      clear();
      if (Number.isFinite(nanoseconds) && nanoseconds >= 0) report(nanoseconds / 1e6, 'available');
    }
    return true;
  }
  function begin() {
    if (active || !prepare()) return;
    if (!extension || query || !gl || gl.isContextLost()) return;
    query = gl.createQuery();
    if (query) { gl.beginQuery(extension.TIME_ELAPSED_EXT, query); active = true; }
  }
  function end() {
    if (active && !gl.isContextLost()) gl.endQuery(extension.TIME_ELAPSED_EXT);
    active = false;
  }
  // Instrumentation must never interrupt the renderer on capability/context loss.
  return {
    prepare() { try { prepare(); } catch { reset(); report(null, 'GPU timer unavailable'); } },
    begin() { try { begin(); } catch { reset(); report(null, 'GPU timer unavailable'); } },
    end() { try { end(); } catch { reset(); report(null, 'GPU timer unavailable'); } },
    reset,
  };
}

// Measure the synchronous CPU cost of actual instance/texture upload API calls.
// Wrappers exist only while sampling is enabled and are restored on disable.
export function observeUploads(sample, draw) {
  const restore = [];
  for (const [type, names, upload] of [
    [globalThis.GPUQueue, ['writeBuffer', 'writeTexture'], true],
    [globalThis.WebGL2RenderingContext, ['bufferData', 'bufferSubData', 'texImage3D', 'texSubImage3D'], true],
    // Start GPU elapsed time at the first real clear, after Rust scene emission
    // and instance uploads, rather than including CPU scene-preparation stalls.
    [globalThis.WebGL2RenderingContext, ['clear', 'clearBufferfv'], false],
  ]) {
    if (!type) continue;
    for (const name of names) {
      const prototype = type.prototype;
      const descriptor = Object.getOwnPropertyDescriptor(prototype, name);
      if (typeof descriptor?.value !== 'function') continue;
      const original = descriptor.value;
      function measured(...args) {
        if (!upload) { draw(); return Reflect.apply(original, this, args); }
        const started = performance.now();
        try { return Reflect.apply(original, this, args); }
        finally { sample(performance.now() - started); }
      }
      try {
        Object.defineProperty(prototype, name, { ...descriptor, value: measured });
        restore.push(() => {
          try {
            if (prototype[name] === measured) Object.defineProperty(prototype, name, descriptor);
          } catch { /* A browser/extension may have locked the entry point. */ }
        });
      } catch { /* Some browsers prohibit replacing native entry points. */ }
    }
  }
  return () => { for (const undo of restore) undo(); };
}
