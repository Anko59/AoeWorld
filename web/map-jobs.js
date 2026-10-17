// Setup owns map preparation. Uncertain acceptance keeps the ORIGINAL key.
// The stated recovery bound (web/start/README.md): a stalled probe releases setup.
const RECOVERY_TIMEOUT_MS = 10_000;
export function initMapJobs({ readJson, controllerHeaders, onStart, onProgress, onActivated, onError, onSettled, onCancelable, onCancelled, onRecovering }) {
  const storageKey = 'aoeworld.map-creation-job';
  let acceptedId = null, submission = null, busy = false, cancelable = false, recovered = false;
  try {
    const saved = JSON.parse(localStorage.getItem(storageKey));
    if (Number.isSafeInteger(saved?.id) && saved.id >= 0) acceptedId = saved.id;
    else if (/^[0-9a-f]{32}$/.test(saved?.submission?.key) && saved.submission.request && typeof saved.submission.request === 'object') submission = saved.submission;
    else localStorage.removeItem(storageKey);
  } catch { /* Storage is optional. */ }
  const pending = () => acceptedId !== null || submission !== null;
  const persist = () => {
    try {
      if (!pending()) localStorage.removeItem(storageKey);
      else localStorage.setItem(storageKey, JSON.stringify({ id: acceptedId, submission }));
    } catch { /* In-memory recovery remains available. */ }
  };
  const remember = id => { acceptedId = id; submission = null; persist(); };
  const controls = () => onCancelable?.(cancelable && acceptedId !== null);
  const submit = async (request, signal) => {
    if (acceptedId !== null) return;
    submission ??= { key: Array.from(crypto.getRandomValues(new Uint8Array(16)), value => value.toString(16).padStart(2, '0')).join(''), request };
    persist();
    const response = await fetch('/maps/jobs', { method: 'POST', headers: {
      'Content-Type': 'application/json', 'Idempotency-Key': submission.key, ...controllerHeaders(),
    }, body: JSON.stringify(submission.request), signal });
    // A lost acknowledgement or a 5xx is ambiguous. Only a definitive refusal
    // permits a fresh key; reloading must never manufacture a second job.
    if (response.status >= 400 && response.status < 500) remember(null);
    const job = await readJson(response);
    if (!Number.isSafeInteger(job.id) || job.id < 0) throw new Error('Invalid map job identifier.');
    remember(job.id);
  };
  const readJob = async signal => {
    const response = await fetch(`/maps/jobs/${acceptedId}`, { signal });
    if (response.status >= 400 && response.status < 500) {
      remember(null);
      throw new Error('This preparation is no longer available. Start a new game.');
    }
    const job = await readJson(response);
    cancelable = !['completed', 'failed', 'cancelled', 'cancel_requested'].includes(job.state);
    controls();
    onProgress?.(job);
    return job;
  };
  const waitForJob = async () => {
    for (;;) {
      const job = await readJob();
      if (job.state === 'completed') return job;
      if (job.state === 'failed' || job.state === 'cancelled') {
        remember(null);
        if (job.state === 'cancelled') { onCancelled?.(); return null; }
        throw new Error(job.error || 'Map preparation failed.');
      }
      await new Promise(resolve => setTimeout(resolve, 250));
    }
  };
  const run = async request => {
    if (busy) return;
    busy = true;
    try {
      onStart?.();
      await submit(request);
      const completed = await waitForJob();
      if (!completed) return;
      const response = await fetch(`/maps/${encodeURIComponent(completed.content_hash)}`, { method: 'POST', headers: controllerHeaders() });
      if (response.status >= 400 && response.status < 500) remember(null);
      const value = await readJson(response);
      remember(null);
      onActivated?.(value);
    } catch (error) {
      onError?.(error.message || 'Unable to prepare this map.', pending());
    } finally {
      busy = false; controls(); onSettled?.();
    }
  };
  const recover = async () => {
    if (recovered || busy) return;
    recovered = true;
    if (!pending()) return;
    busy = true;
    onRecovering?.(true);
    // A timeout is "retry later": the saved key or accepted ID is kept, and
    // only a definitive 4xx refusal (handled in submit/readJob) clears it.
    const controller = new AbortController();
    let timedOut = false;
    const timer = setTimeout(() => { timedOut = true; controller.abort(); }, RECOVERY_TIMEOUT_MS);
    try {
      // Probe exactly once after acquiring the controller. Adopt acceptance,
      // but let Start/Resume own loading and activation, including Cancel.
      await submit(null, controller.signal);
      const job = await readJob(controller.signal);
      if (['failed', 'cancelled'].includes(job.state)) remember(null);
    } catch (error) {
      onError?.(timedOut ? 'Map recovery timed out. Your saved request is kept; try again.' : error.message || 'Unable to recover this map request.', pending());
    } finally {
      clearTimeout(timer);
      busy = false; controls(); onRecovering?.(false); onSettled?.();
    }
  };
  const cancel = async () => {
    if (acceptedId === null || !cancelable) return;
    cancelable = false; controls();
    try {
      await readJson(await fetch(`/maps/jobs/${acceptedId}/cancel`, { method: 'POST', headers: controllerHeaders() }));
      if (!busy) run(null);
    } catch (error) { cancelable = true; controls(); throw error; }
  };
  controls();
  return { start: run, cancel, recover, pending };
}
