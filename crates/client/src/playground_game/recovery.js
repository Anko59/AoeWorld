const reasonKey = 'aoeworld.renderer-recovery';

export function report() {
  try {
    const reason = sessionStorage.getItem(reasonKey);
    if (reason !== null) {
      console.warn(`WebGPU failed: ${reason}; falling back to WebGL2`);
      sessionStorage.removeItem(reasonKey);
    }
  } catch { /* Storage may be disabled; recovery still works. */ }
}

export function restart_renderer(reason, view) {
  console.error(`WebGPU failed: ${reason}; falling back to WebGL2 (restarting renderer)`);
  try { sessionStorage.setItem(reasonKey, Array.from(reason).slice(0, 4096).join('')); }
  catch { /* A fresh document can recover without persisted diagnostics. */ }
  const params = new URLSearchParams(location.hash.slice(1));
  // Camera links do not follow panning/zooming. Snapshot the live view;
  // startup failures have no view yet and retain the requested fragment.
  ['x', 'y', 'zoom'].forEach((key, index) => {
    if (view.length) params.set(key, String(view[index]));
  });
  params.set('renderer', 'webgl2');
  location.hash = params.toString();
  location.reload();
}
