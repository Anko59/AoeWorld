import { colours, presets, views } from './views.js';
import { createCinematic } from './cinematic.js';

const introKey = 'aoeworld.intro-seen';
const colourKey = 'aoeworld.player-colour';
const setupKey = 'aoeworld.skirmish-map';
const tips = [
  'Use the minimap to find your way across the landscape.',
  'Select your unit, then right-click on land to give a move order.',
  'Press M to unfold the whole map. Press Home to return to your horse.',
  'Hold Shift while using the arrow keys to travel across the map faster.',
  'A larger compression brings more of the real world into a smaller battlefield.',
];
const phaseNames = {
  queued: 'Waiting to prepare your map', acquiring_sources: 'Gathering landscape sources',
  downloading_source: 'Downloading a landscape source', preparing_source: 'Preparing the landscape',
  preparing_detailed: 'Preparing regional elevation', building_pyramids: 'Building terrain detail',
  verifying_package: 'Checking the prepared landscape', generating: 'Creating your landscape',
};
const getSaved = key => { try { return sessionStorage.getItem(key); } catch { return null; } };
const save = (key, value) => { try { sessionStorage.setItem(key, value); } catch { /* Memory remains usable. */ } };

export function initStartSequence({ mapRequest, readJson, start, cancel }) {
  const root = document.getElementById('start-sequence');
  const direct = new URLSearchParams(location.search).get('play') === '1'
    || /(?:^#|&)\s*(?:x|y|zoom|renderer)=/.test(location.hash);
  const el = id => document.getElementById(id);
  function gameColour(name) {
    const selected = colours.find(([value]) => value === name) ?? colours[0];
    document.documentElement.style.setProperty('--player-colour', selected[1]);
    el('playground').dataset.playerColour = selected[0];
    el('minimap-primary').setAttribute('fill', selected[1]);
    save(colourKey, selected[0]);
    return selected;
  }
  if (direct) {
    // Camera/capture entry does not construct or download the menu artwork.
    root.hidden = true;
    document.body.dataset.startState = 'game';
    el('playground').inert = false;
    gameColour(getSaved(colourKey));
    const idle = () => {};
    return { begin: idle, recover: idle, progress: idle, activated: idle, error: idle, settled: idle, cancelable: idle, cancelled: idle };
  }
  root.innerHTML = views();
  let state = 'intro';
  let estimateRevision = 0;
  let estimateTimer;
  const cinematic = createCinematic(root, skipIntro);
  let tipTimer;
  let terrainTimer;
  let waiting = null;
  let recovery = false;
  let jobRunning = false;
  let prepared = false;
  let skirmishJob = false;
  let canCancel = false;
  const fields = ['start-latitude', 'start-longitude', 'start-size', 'start-scale'];
  const creatorFields = ['map-latitude', 'map-longitude', 'map-side-km', 'map-compression'];

  function show(next) {
    state = next;
    root.dataset.state = next;
    document.body.dataset.startState = next;
    el('playground').inert = next !== 'game';
    for (const [id, name] of [['entry-intro', 'intro'], ['main-menu', 'menu'], ['skirmish-setup', 'setup'], ['loading-screen', 'loading']]) {
      el(id).hidden = next !== name;
    }
    root.hidden = next === 'game';
    if (next === 'menu') el('single-player').focus();
    if (next === 'game') el('scene').focus();
  }
  function applyColour(name) {
    const [selected] = gameColour(name);
    const radio = root.querySelector(`input[value="${selected}"]`);
    radio.checked = true;
    el('colour-name').textContent = selected[0].toUpperCase() + selected.slice(1);
  }
  applyColour(getSaved(colourKey));
  for (const radio of root.querySelectorAll('input[name="player-colour"]')) {
    radio.addEventListener('change', () => applyColour(radio.value));
  }
  try {
    const saved = JSON.parse(getSaved(setupKey));
    if (Array.isArray(saved) && saved.length === 4 && saved.every(value => typeof value === 'string')) {
      fields.forEach((id, index) => { el(id).value = saved[index]; });
    }
  } catch { /* Invalid saved setup uses the default. */ }

  if (!el('start-size').value) el('start-size').value = '480';

  function syncRequest() {
    if (!el('skirmish-form').checkValidity()) throw new Error('Choose valid coordinates, a square size and a scale.');
    fields.forEach((id, index) => { el(creatorFields[index]).value = el(id).value; });
    // Skirmish exposes colour and geography only. Source detail, rivers and
    // deterministic generation use the creator's current automatic policy.
    el('map-preparation').value = 'automatic';
    el('map-preparation').dispatchEvent(new Event('change', { bubbles: true }));
    el('map-hydrology').value = 'automatic';
    el('map-seed').value = '1';
    const preset = presets.find(([, , , latitude, longitude]) =>
      Number(el(fields[0]).value) === latitude && Number(el(fields[1]).value) === longitude);
    const region = el('map-region');
    region.value = preset && Array.from(region.options).some(option => option.value === preset[0])
      ? preset[0] : 'custom';
    region.dispatchEvent(new Event('change', { bubbles: true }));
    return mapRequest();
  }
  function locationName() {
    const match = presets.find(([, , , latitude, longitude]) =>
      Number(el(fields[0]).value) === latitude && Number(el(fields[1]).value) === longitude);
    for (const button of root.querySelectorAll('[data-preset]')) {
      button.setAttribute('aria-pressed', String(button.dataset.preset === match?.[0]));
    }
    el('chosen-location').textContent = match?.[1] ?? 'Custom location';
    return match?.[1] ?? `${el(fields[0]).value}°, ${el(fields[1]).value}°`;
  }
  async function estimateMap() {
    const revision = ++estimateRevision;
    locationName();
    save(setupKey, JSON.stringify(fields.map(id => el(id).value)));
    try {
      const request = syncRequest();
      el('skirmish-estimate').textContent = 'Estimating your landscape…';
      const value = await readJson(await fetch('/maps/estimate', {
        method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(request),
      }));
      if (revision !== estimateRevision) return;
      const square = (value.game_side_meters / 1000).toLocaleString(undefined, { maximumFractionDigits: 2 });
      const minutes = Math.max(1, Math.round(value.walking_crossing_seconds / 60));
      const sources = value.preparation?.mode === 'procedural_fallback'
        ? 'Terrain sources are unavailable; this map will use generated terrain.'
        : 'Rivers are included where source coverage permits.';
      el('skirmish-estimate').textContent = `${Number(el('start-size').value).toLocaleString()} km of real landscape per side, compressed to ${square} km in game. About ${minutes.toLocaleString()} minutes to walk across. ${sources}`;
    } catch (error) {
      if (revision === estimateRevision) el('skirmish-estimate').textContent = error.message || 'The map estimate is unavailable. Try another location or size.';
    }
  }
  function scheduleEstimate() {
    ++estimateRevision;
    clearTimeout(estimateTimer);
    locationName();
    estimateTimer = setTimeout(estimateMap, 300);
  }
  fields.forEach(id => el(id).addEventListener('input', scheduleEstimate));
  for (const button of root.querySelectorAll('[data-preset]')) {
    button.addEventListener('click', () => {
      const preset = presets.find(([id]) => id === button.dataset.preset);
      el(fields[0]).value = String(preset[3]);
      el(fields[1]).value = String(preset[4]);
      estimateMap();
    });
  }
  el('custom-location').addEventListener('click', () => { el(fields[0]).focus(); el(fields[0]).select(); });
  el('single-player').addEventListener('click', () => { show('setup'); estimateMap(); });
  el('setup-back').addEventListener('click', () => show('menu'));
  function controllerReady() {
    const connected = el('connection').textContent === 'connected';
    const controller = /^[0-9a-f]{48}$/i.test(getSaved('aoeworld.resume-token') || '');
    el('start-game').disabled = !connected || !controller || jobRunning;
    el('start-game').firstChild.textContent = recovery ? 'Resume map preparation ' : 'Start Game ';
    el('setup-status').textContent = connected
      ? !controller ? 'Another tab controls this world. Close it, then reload this page.'
        : recovery ? 'Your saved request will resume safely.' : 'Your world, your starting place.'
      : el('connection').textContent.includes('disconnected')
        ? 'Connection lost. Reload this page to reconnect.' : 'Connecting to your world…';
  }
  el('skirmish-form').addEventListener('submit', event => {
    event.preventDefault();
    try {
      start(syncRequest());
      // Resume adopts a retained job even when it is already polling.
      begin();
    } catch (error) { el('setup-status').textContent = error.message; }
  });
  function progress(completed, total, label) {
    const bar = el('loading-progress');
    const measured = Number.isSafeInteger(completed) && Number.isSafeInteger(total) && total > 0;
    bar.classList.toggle('indeterminate', !measured);
    bar.setAttribute('aria-valuetext', label);
    if (measured) {
      bar.setAttribute('aria-valuemin', '0');
      bar.setAttribute('aria-valuenow', String(completed));
      bar.setAttribute('aria-valuemax', String(total));
      bar.firstElementChild.style.width = `${Math.min(100, completed / total * 100)}%`;
    } else {
      for (const attr of ['aria-valuemin', 'aria-valuenow', 'aria-valuemax']) bar.removeAttribute(attr);
      bar.firstElementChild.style.width = '';
    }
    el('loading-phase').textContent = label;
  }
  function begin() {
    // Only setup starts the entry flow. The in-game creator keeps its own UI.
    if (state !== 'setup') return;
    skirmishJob = true;
    jobRunning = true;
    prepared = false;
    waiting = null;
    clearTimeout(terrainTimer);
    cinematic.stop();
    clearInterval(tipTimer);
    el('loading-error').hidden = el('loading-back').hidden = true;
    el('loading-cancel').hidden = false;
    el('loading-cancel').disabled = !canCancel;
    el('loading-screen').classList.remove('has-error');
    el('loading-progress').hidden = false;
    el('loading-title').textContent = 'Preparing your landscape';
    el('loading-location').textContent = `${locationName()} · ${el('start-size').value} km square`;
    progress(null, null, recovery ? 'Resuming your map preparation…' : 'Submitting your map request…');
    show('loading');
    let tip = 0;
    el('loading-tip').textContent = tips[0];
    tipTimer = setInterval(() => { el('loading-tip').textContent = tips[++tip % tips.length]; }, 6000);
  }
  function error(message, resumable = false) {
    if (!skirmishJob) return;
    waiting = null;
    jobRunning = false;
    recovery = resumable;
    clearTimeout(terrainTimer);
    clearInterval(tipTimer);
    show('loading');
    el('loading-title').textContent = 'Your world needs another moment';
    const explanation = /starting-location search limit/i.test(message)
      ? 'The game could not find a safe starting place in this landscape. Try another location, or return when terrain sources are available.'
      : /Failed to fetch|NetworkError|Load failed/i.test(message)
        ? 'The map service could not be reached. Check your connection, then try again.' : message;
    el('loading-error').textContent = `We couldn't open your landscape. ${explanation}`;
    el('loading-error').hidden = el('loading-back').hidden = false;
    el('loading-cancel').hidden = true;
    el('loading-screen').classList.add('has-error');
    el('loading-progress').hidden = true;
    el('loading-phase').textContent = 'Return to setup to try again.';
    el('loading-back').focus();
    controllerReady();
  }
  function checkTerrain() {
    controllerReady();
    if (!waiting) return;
    const scene = el('scene');
    const minimap = el('minimap');
    const sameMap = minimap.dataset.mapHash === waiting.hash;
    const terrain = sameMap && Number(scene.dataset.frame) > 0 && scene.dataset.chunksPending === '0'
      && scene.dataset.chunksFailed === '0' && scene.dataset.frame !== waiting.frame;
    const raster = sameMap && minimap.dataset.raster === 'ready';
    progress(Number(terrain) + Number(raster), 2,
      terrain && raster ? 'Your landscape is ready' : 'Opening terrain · waiting for the first landscape and minimap');
    if (terrain && raster && el('connection').textContent === 'connected') {
      const primary = el('minimap-primary');
      const start = [Number(primary.getAttribute('cx')) * Number(minimap.dataset.worldWidth) / 256,
        Number(primary.getAttribute('cy')) * Number(minimap.dataset.worldHeight) / 256];
      const camera = [Number(minimap.dataset.cameraX), Number(minimap.dataset.cameraY)];
      if (primary.getAttribute('visibility') !== 'visible' || !start.every(Number.isFinite)
        || !camera.every(Number.isFinite)) return;
      // A geographic map's centre may be water. Use the game's own start
      // control, then keep the painting up until that view has presented.
      if (!waiting.centered && Math.hypot(camera[0] - start[0], camera[1] - start[1]) > 0.01) {
        waiting.centered = true;
        waiting.frame = scene.dataset.frame;
        el('recenter').click();
        return;
      }
      waiting = null;
      jobRunning = false;
      prepared = false;
      skirmishJob = false;
      clearTimeout(terrainTimer);
      clearInterval(tipTimer);
      el('playground').inert = false;
      document.body.dataset.startState = 'game';
      root.classList.add('entering-game');
      setTimeout(() => { show('game'); root.classList.remove('entering-game'); }, 650);
    }
  }
  const observer = new MutationObserver(checkTerrain);
  observer.observe(el('scene'), { attributes: true, attributeFilter: ['data-chunks-pending', 'data-chunks-failed', 'data-frame'] });
  observer.observe(el('minimap'), { attributes: true, attributeFilter: ['data-raster', 'data-map-hash'] });
  observer.observe(el('connection'), { childList: true, subtree: true, characterData: true });
  function returnToSetup() {
    skirmishJob = false;
    waiting = null;
    jobRunning = prepared = false;
    clearTimeout(terrainTimer);
    clearInterval(tipTimer);
    show('setup');
    controllerReady();
  }
  el('loading-back').addEventListener('click', returnToSetup);
  el('loading-cancel').addEventListener('click', async () => {
    el('loading-error').hidden = true;
    try { await cancel(); }
    catch (error) {
      if (!skirmishJob) return;
      el('loading-error').textContent = error.message || 'Unable to cancel map creation. Try again.';
      el('loading-error').hidden = false;
    }
  });

  function skipIntro() {
    if (state !== 'intro') return;
    cinematic.stop();
    save(introKey, '1');
    show('menu');
  }
  document.addEventListener('keydown', event => {
    if (state === 'game') return;
    if (state === 'intro') { event.preventDefault(); skipIntro(); }
    event.stopImmediatePropagation();
  }, true);
  root.addEventListener('pointerdown', event => {
    if (state === 'intro') { event.preventDefault(); event.stopPropagation(); skipIntro(); }
  });
  el('skip-cinematic').addEventListener('click', skipIntro);
  if (direct) show('game');
  else if (getSaved(introKey) || new URLSearchParams(location.search).get('skipIntro') === '1') show('menu');
  else {
    show('intro');
    save(introKey, '1');
    cinematic.start();
  }
  locationName();
  controllerReady();
  return {
    begin,
    recover(value) { recovery = value; controllerReady(); },
    progress(job) {
      if (!skirmishJob) return;
      const request = job.request;
      if (request && request.compression?.denominator > 0) {
        const values = [request.center_latitude_e7 / 1e7, request.center_longitude_e7 / 1e7,
          request.requested_side_meters / 1000, request.compression.numerator / request.compression.denominator];
        if (values.every(Number.isFinite)) {
          fields.forEach((id, index) => { el(id).value = String(values[index]); });
          el('loading-location').textContent = `${locationName()} · ${el('start-size').value} km square`;
        }
      }
      if (['completed', 'failed', 'cancelled'].includes(job.state)) {
        el('loading-cancel').hidden = true;
        return;
      }
      const phase = job.progress?.phase || job.stage || 'queued';
      const title = phaseNames[phase] ?? phase.replaceAll('_', ' ').replace(/^./, letter => letter.toUpperCase());
      const { completed, total, unit } = job.progress ?? {};
      const counts = Number.isSafeInteger(completed) && Number.isSafeInteger(total) && total > 0
        ? ` · ${completed.toLocaleString()} / ${total.toLocaleString()} ${unit || 'steps'}` : '';
      progress(completed, total, title + counts);
    },
    activated(value) {
      if (!skirmishJob) return;
      recovery = false;
      prepared = true;
      if (!value.start_available) { error(value.message || 'This location has no suitable land start. Choose another place.'); return; }
      waiting = { hash: value.content_hash, frame: el('scene').dataset.frame, centered: false };
      el('loading-title').textContent = 'Opening your world';
      el('reconnect').click();
      terrainTimer = setTimeout(() => error('The terrain could not finish loading. Check your connection, then try again.'), 90000);
      checkTerrain();
    },
    error,
    cancelable(value) { canCancel = value; el('loading-cancel').disabled = !value; },
    cancelled() {
      if (!skirmishJob) return;
      recovery = false;
      returnToSetup();
    },
    settled() { if (!prepared) jobRunning = false; controllerReady(); },
  };
}
