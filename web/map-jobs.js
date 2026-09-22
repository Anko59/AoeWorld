export function initMapJobs({ readJson, controllerHeaders, preparationSummary, mapRequest, refreshPackages, estimate, generate, retry, cancel }) {
  const historySelect = document.getElementById('job-history');
  const historyRetry = document.getElementById('retry-history');
  const storageKey = 'aoeworld.map-creation-job';
  let acceptedId = null;
  let submission = null;
  let retryRequest = null;
  let previousJobs = [];
  let busy = false;
  try {
    const saved = JSON.parse(localStorage.getItem(storageKey));
    if (Number.isSafeInteger(saved) && saved >= 0) acceptedId = saved;
    else if (saved && typeof saved === 'object') {
      if (Number.isSafeInteger(saved.id) && saved.id >= 0) acceptedId = saved.id;
      else if (/^[0-9a-f]{32}$/.test(saved.submission?.key) && saved.submission.request) {
        submission = saved.submission;
        retryRequest = submission.request;
      }
    }
  } catch { /* Storage may be unavailable; server history remains usable. */ }
  const persist = () => {
    try {
      if (acceptedId === null && submission === null) localStorage.removeItem(storageKey);
      else localStorage.setItem(storageKey, JSON.stringify({ id: acceptedId, submission }));
    } catch { /* In-memory retries still reuse the same key. */ }
  };
  const remember = id => {
    acceptedId = id;
    submission = null;
    persist();
  };
  const newSubmission = request => ({
    key: Array.from(crypto.getRandomValues(new Uint8Array(16)), value => value.toString(16).padStart(2, '0')).join(''),
    request,
  });
  const originalRequest = job => ({ ...job.request, preparation: job.preparation.mode === 'procedural_fallback' ? 'automatic' : job.preparation.mode });
  const controls = () => {
    generate.disabled = busy || acceptedId !== null || submission !== null;
    retry.disabled = busy || (acceptedId === null && retryRequest === null);
    retry.textContent = acceptedId !== null ? 'Resume job' : submission !== null ? 'Recover request' : 'Retry';
    cancel.disabled = acceptedId === null;
    historyRetry.disabled = busy || acceptedId !== null || submission !== null || !historySelect.value;
    const selected = previousJobs.find(job => String(job.id) === historySelect.value);
    historyRetry.textContent = selected && !['failed', 'cancelled'].includes(selected.state) ? 'Resume selected job' : 'Retry selected request';
  };
  const refreshHistory = async () => {
    previousJobs = await readJson(await fetch('/maps/jobs'));
    historySelect.replaceChildren(new Option('No previous request selected', ''));
    for (const job of previousJobs.slice().reverse()) {
      const r = job.request;
      historySelect.add(new Option(`#${job.id}: ${job.state} · ${r.center_latitude_e7 / 1e7}, ${r.center_longitude_e7 / 1e7} · ${r.requested_side_meters / 1000} km`, String(job.id)));
    }
    controls();
  };
  const waitForJob = async id => {
    for (;;) {
      const response = await fetch(`/maps/jobs/${id}`);
      if (response.status === 404) {
        remember(null);
        throw new Error('This job is no longer retained. Select a saved map or retry the request.');
      }
      const job = await readJson(response);
      retryRequest = originalRequest(job);
      if (job.state === 'completed') return job;
      if (job.state === 'failed' || job.state === 'cancelled') {
        remember(null);
        throw new Error(job.error || 'Map creation cancelled.');
      }
      const progress = job.progress;
      const phase = progress?.phase || job.stage;
      const count = progress && Number.isSafeInteger(progress.completed) && Number.isSafeInteger(progress.total)
        ? ` · ${progress.completed.toLocaleString()} / ${progress.total.toLocaleString()} ${progress.unit}` : '';
      const scope = phase === 'downloading_source' ? ' (current source)' : '';
      estimate.className = '';
      estimate.textContent = `${phase.replaceAll('_', ' ')}${scope}${count}\n${preparationSummary(job.preparation)}`;
      await new Promise(resolve => setTimeout(resolve, 250));
    }
  };
  const run = async (request, existingId = acceptedId) => {
    if (busy) return;
    busy = true;
    controls();
    try {
      if (existingId === null) {
        submission ??= newSubmission(request);
        retryRequest = submission.request;
        persist();
        const response = await fetch('/maps/jobs', { method: 'POST', headers: { 'Content-Type': 'application/json', 'Idempotency-Key': submission.key, ...controllerHeaders() }, body: JSON.stringify(submission.request) });
        // These replies explicitly reject acceptance; transport/5xx uncertainty
        // retains the key so recovery cannot submit a second retained job.
        if (response.status === 400 || response.status === 429) { submission = null; persist(); }
        const job = await readJson(response);
        if (!Number.isSafeInteger(job.id) || job.id < 0) throw new Error('Invalid map job identifier.');
        remember(job.id);
      } else {
        remember(existingId);
      }
      controls();
      const completed = await waitForJob(acceptedId);
      const value = await readJson(await fetch(`/maps/${encodeURIComponent(completed.content_hash)}`, { method: 'POST', headers: controllerHeaders() }));
      remember(null);
      retryRequest = null;
      await refreshPackages();
      estimate.className = '';
      const source = value.uses_fallback_data ? 'Fallback' : 'Source-backed';
      estimate.textContent = value.start_available
        ? `${source} package active: ${value.tiles_per_side.toLocaleString()} × ${value.tiles_per_side.toLocaleString()} tiles\nPackage ${value.content_hash.slice(0, 12)}… · ${value.source_lock_count} verified sources\nReconnect to join the activated world.`
        : `${source} package available for preview: ${value.tiles_per_side.toLocaleString()} × ${value.tiles_per_side.toLocaleString()} tiles\n${value.message || 'No suitable land start.'} Open its terrain preview; gameplay was not replaced.`;
      retryRequest = null;
    } catch (error) {
      estimate.className = 'error';
      const resume = acceptedId !== null ? ' Resume this job to check its status; no new job will be submitted.' : submission !== null ? ' Recover this request using its saved submission key.' : '';
      estimate.textContent = (error.message || 'Unable to complete map creation.') + resume;
    } finally {
      busy = false;
      controls();
      refreshHistory().catch(() => { historyRetry.disabled = true; });
    }
  };
  generate.addEventListener('click', () => {
    try { run(mapRequest()); }
    catch (error) { estimate.className = 'error'; estimate.textContent = error.message; }
  });
  retry.addEventListener('click', () => run(retryRequest));
  historySelect.addEventListener('change', controls);
  historyRetry.addEventListener('click', () => {
    const job = previousJobs.find(value => String(value.id) === historySelect.value);
    if (!job) return;
    retryRequest = originalRequest(job);
    run(retryRequest, ['failed', 'cancelled'].includes(job.state) ? null : job.id);
  });
  cancel.addEventListener('click', async () => {
    if (acceptedId === null) return;
    cancel.disabled = true;
    try {
      await readJson(await fetch(`/maps/jobs/${acceptedId}/cancel`, { method: 'POST', headers: controllerHeaders() }));
      if (!busy) run(null);
    } catch (error) {
      estimate.className = 'error';
      estimate.textContent = error.message || 'Unable to cancel map creation.';
      controls();
    }
  });
  controls();
  if (acceptedId !== null) run(null);
  return { refreshHistory };
}
