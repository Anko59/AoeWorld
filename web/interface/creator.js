import { initInterface } from '/interface/hud.js';
import init from '/pkg/aoe_client.js';
import { initMapJobs } from '/map-jobs.js';
import { initNavigation } from '/navigation.js';
import { initStartSequence } from '/start/sequence.js';
initInterface();
initNavigation();
const creator = document.getElementById('map-creator');
const estimate = document.getElementById('map-estimate');
const generate = document.getElementById('generate-map');
const retry = document.getElementById('retry-map');
const cancel = document.getElementById('cancel-map');
const packageSelect = document.getElementById('map-package');
const previewLayer = document.getElementById('preview-layer');
const terrainPreview = document.getElementById('terrain-preview');
const terrainPreviewCanvas = document.getElementById('terrain-preview-canvas');
const terrainPreviewSummary = document.getElementById('terrain-preview-summary');
const region = document.getElementById('map-region');
const coordinates = document.getElementById('map-coordinates');
const footprint = document.getElementById('map-footprint');
const overview = document.getElementById('map-overview');
const location = document.getElementById('map-location');
const regions = Object.freeze({
  paris: { name: 'Paris Basin', latitude: 48.8566, longitude: 2.3522 },
  alps: { name: 'Alps', latitude: 46.6, longitude: 10.5 },
  tiber: { name: 'Tiber Valley', latitude: 41.9028, longitude: 12.4964 },
  constantinople: { name: 'Bosporus', latitude: 41.0082, longitude: 28.9784 },
  nile: { name: 'Nile Delta', latitude: 30.0444, longitude: 31.2357 },
  niger: { name: 'Niger Inland Delta', latitude: 14.4974, longitude: -4.1956 },
  finland: { name: 'Southern Finland', latitude: 61.5, longitude: 25.5 },
  yangtze: { name: 'Lower Yangtze', latitude: 31.2304, longitude: 121.4737 },
  borneo: { name: 'Borneo', latitude: 0.5, longitude: 114.0 },
  sahara: { name: 'Sahara', latitude: 24.0, longitude: 10.0 },
  fiji: { name: 'Fiji', latitude: -17.8, longitude: 179.8 },
  antarctic: { name: 'Antarctic interior', latitude: -80.0, longitude: 0.0 },
  continental: { name: 'Continental Europe/Africa', latitude: 35.0, longitude: 15.0 }
});
let currentTerrainPreview = null;
const overviewView = { x: 0, y: 0, width: 360, height: 180 };
let overviewDrag = null;
let footprintRevision = 0;
const number = id => Number(document.getElementById(id).value);
const clampOverview = () => {
  overviewView.width = Math.min(360, Math.max(18, overviewView.width));
  overviewView.height = overviewView.width / 2;
  overviewView.x = Math.min(360 - overviewView.width, Math.max(0, overviewView.x));
  overviewView.y = Math.min(180 - overviewView.height, Math.max(0, overviewView.y));
  overview.setAttribute('viewBox', `${overviewView.x} ${overviewView.y} ${overviewView.width} ${overviewView.height}`);
};
const overviewPoint = event => {
  const bounds = overview.getBoundingClientRect();
  return {
    x: overviewView.x + (event.clientX - bounds.left) * overviewView.width / bounds.width,
    y: overviewView.y + (event.clientY - bounds.top) * overviewView.height / bounds.height
  };
};
const chooseOverviewCenter = point => {
  region.value = 'custom';
  coordinates.hidden = false;
  document.getElementById('map-latitude').value = String(Math.max(-90, Math.min(90, 90 - point.y)));
  document.getElementById('map-longitude').value = String(Math.max(-180, Math.min(180, point.x - 180)));
  updatePreview();
};
const overviewCoordinate = point => ({ x: point.longitude_e7 / 1e7 + 180, y: 90 - point.latitude_e7 / 1e7 });
const drawGeographicFootprint = points => {
  let path = '';
  let previous = null;
  let split = false;
  for (const point of points) {
    const coordinate = overviewCoordinate(point);
    if (previous === null || Math.abs(coordinate.x - previous.x) > 180) {
      path += `M${coordinate.x} ${coordinate.y}`;
      split ||= previous !== null;
    } else {
      path += `L${coordinate.x} ${coordinate.y}`;
    }
    previous = coordinate;
  }
  footprint.setAttribute('d', split ? path : `${path}Z`);
};
const drawFallbackFootprint = (latitude, longitude, sideKm) => {
  const normalizedLongitude = ((longitude + 180) % 360 + 360) % 360 - 180;
  const latitudeSpan = Math.min(180, Math.max(0.05, sideKm / 111.32));
  const longitudeSpan = Math.min(360, Math.max(0.05, sideKm / (111.32 * Math.max(0.1, Math.cos(latitude * Math.PI / 180)))));
  const x = Math.min(360, Math.max(0, normalizedLongitude + 180 - longitudeSpan / 2));
  const y = Math.min(180, Math.max(0, 90 - latitude - latitudeSpan / 2));
  footprint.setAttribute('d', `M${x} ${y}h${longitudeSpan}v${latitudeSpan}h-${longitudeSpan}Z`);
};
const footprintSummary = (latitude, longitude, sideKm) => {
  const normalizedLongitude = ((longitude + 180) % 360 + 360) % 360 - 180;
  const selected = regions[region.value];
  const label = selected ? selected.name : 'Custom coordinates';
  return `${label}: ${latitude.toFixed(4)}°, ${normalizedLongitude.toFixed(4)}° · ${sideKm.toLocaleString()} km square footprint`;
};
const distortionSummary = distortion => {
  if (!Number.isInteger(distortion?.min_scale_error_ppm) || !Number.isInteger(distortion?.max_scale_error_ppm)) return '';
  const ppm = value => `${value >= 0 ? '+' : ''}${value.toLocaleString()} ppm`;
  return ` · Projection scale error ${ppm(distortion.min_scale_error_ppm)} to ${ppm(distortion.max_scale_error_ppm)}`;
};
const updatePreview = () => {
  const latitude = number('map-latitude');
  const longitude = number('map-longitude');
  const sideKm = number('map-side-km');
  const valid = Number.isFinite(latitude) && Number.isFinite(longitude) && Number.isFinite(sideKm);
  if (!valid) {
    footprint.setAttribute('visibility', 'hidden');
    location.textContent = 'Enter a valid location and square size to preview the geographic footprint.';
    return;
  }
  footprint.setAttribute('visibility', 'visible');
  drawFallbackFootprint(latitude, longitude, sideKm);
  const revision = ++footprintRevision;
  try {
    const request = mapRequest();
    fetch('/maps/footprint', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(request) })
      .then(response => response.ok ? response.json() : Promise.reject())
      .then(value => {
        if (revision === footprintRevision && Array.isArray(value.points) && value.points.length > 3) {
          drawGeographicFootprint(value.points);
          location.textContent = `${footprintSummary(latitude, longitude, sideKm)}${distortionSummary(value.distortion)}`;
        }
      })
      .catch(() => {});
  } catch (_) {}
  location.textContent = footprintSummary(latitude, longitude, sideKm);
};
const selectRegion = () => {
  const selected = regions[region.value];
  coordinates.hidden = Boolean(selected);
  if (selected) {
    document.getElementById('map-latitude').value = String(selected.latitude);
    document.getElementById('map-longitude').value = String(selected.longitude);
  }
  updatePreview();
};
const controllerHeaders = () => {
  const token = sessionStorage.getItem('aoeworld.resume-token');
  if (!/^[0-9a-f]{48}$/i.test(token || '')) throw new Error('Map creation requires the current game controller. Reconnect, then try again.');
  return { 'X-AoeWorld-Controller-Token': token };
};
const ratio = value => {
  const text = String(value).trim();
  if (!/^\d+(\.\d{1,3})?$/.test(text)) throw new Error('Compression must be 1:1 through 10,000:1 with up to three decimals.');
  const [whole, fraction = ''] = text.split('.');
  const scale = 10 ** fraction.length;
  return { numerator: Number(whole) * scale + Number(fraction.padEnd(fraction.length, '0') || 0), denominator: scale };
};
const formatTime = seconds => {
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  return hours ? `${hours} h ${minutes} min` : `${minutes} min ${seconds % 60} s`;
};
const formatCompression = compression => {
  const value = compression.numerator / compression.denominator;
  return `${value.toLocaleString(undefined, { maximumFractionDigits: 3 })}:1`;
};
const previewColor = (cell, preview) => {
  if (previewLayer.value === 'water') return cell.water ? [43, 117, 166] : [73, 105, 55];
  if (previewLayer.value === 'vegetation') {
    const palette = [[105, 129, 69], [57, 104, 58], [166, 151, 79], [85, 122, 104], [151, 139, 103], [121, 144, 166]];
    return palette[cell.biome % palette.length];
  }
  if (previewLayer.value === 'suitability') return cell.passable ? [112, 156, 81] : [62, 72, 77];
  const span = Math.max(1, preview.maximum_height_centimeters - preview.minimum_height_centimeters);
  const shade = Math.round(38 + 190 * (cell.geographic_height_centimeters - preview.minimum_height_centimeters) / span);
  return [shade, Math.round(shade * 0.91), Math.round(shade * 0.67)];
};
const drawTerrainPreview = preview => {
  if (!preview || !Array.isArray(preview.cells) || !Number.isInteger(preview.samples_per_axis)) return;
  const context = terrainPreviewCanvas.getContext('2d');
  const image = context.createImageData(terrainPreviewCanvas.width, terrainPreviewCanvas.height);
  const cellSize = terrainPreviewCanvas.width / preview.samples_per_axis;
  for (let y = 0; y < preview.samples_per_axis; y++) for (let x = 0; x < preview.samples_per_axis; x++) {
    const color = previewColor(preview.cells[y * preview.samples_per_axis + x], preview);
    for (let pixelY = Math.floor(y * cellSize); pixelY < Math.floor((y + 1) * cellSize); pixelY++) for (let pixelX = Math.floor(x * cellSize); pixelX < Math.floor((x + 1) * cellSize); pixelX++) {
      const offset = (pixelY * terrainPreviewCanvas.width + pixelX) * 4;
      image.data.set([...color, 255], offset);
    }
  }
  context.putImageData(image, 0, 0);
  const source = preview.source_backed ? 'Source-backed' : 'Fallback';
  terrainPreviewSummary.textContent = `${source} coarse ${preview.samples_per_axis} × ${preview.samples_per_axis} generated terrain sample · elevation ${(preview.minimum_height_centimeters / 100).toLocaleString()}–${(preview.maximum_height_centimeters / 100).toLocaleString()} m. Passable ground is a terrain diagnostic, not a selected start.`;
  terrainPreview.hidden = false;
};
const mapRequest = () => {
  const side = number('map-side-km');
  return {
    schema_version: 1,
    center_latitude_e7: Math.round(number('map-latitude') * 1e7),
    center_longitude_e7: Math.round(number('map-longitude') * 1e7),
    requested_side_meters: Math.round(side * 1000),
    compression: ratio(document.getElementById('map-compression').value),
    year_ce: 600,
    seed: Math.round(number('map-seed')),
    reconstruction_profile: 'circa600_v1',
    preparation: document.getElementById('map-preparation').value,
    hydrology_mode: document.getElementById('map-hydrology').value
  };
};
document.getElementById('map-preparation').addEventListener('change', event => {
  const rivers = document.getElementById('map-hydrology');
  const detailed = event.target.value === 'detailed';
  if (detailed && !rivers.disabled) rivers.dataset.chosen = rivers.value;
  if (!detailed && rivers.disabled) rivers.value = rivers.dataset.chosen ?? 'automatic';
  rivers.disabled = detailed;
  if (detailed) rivers.value = 'vectors';
});
region.addEventListener('change', selectRegion);
for (const id of ['map-latitude', 'map-longitude', 'map-side-km']) document.getElementById(id).addEventListener('input', updatePreview);
overview.addEventListener('pointerdown', event => {
  if (event.button !== 0) return;
  overview.setPointerCapture(event.pointerId);
  const point = overviewPoint(event);
  overviewDrag = event.shiftKey
    ? { mode: 'pan', x: event.clientX, y: event.clientY }
    : { mode: 'select' };
  if (overviewDrag.mode === 'select') chooseOverviewCenter(point);
  event.preventDefault();
});
overview.addEventListener('pointermove', event => {
  if (!overviewDrag) return;
  const point = overviewPoint(event);
  if (overviewDrag.mode === 'pan') {
    const bounds = overview.getBoundingClientRect();
    overviewView.x -= (event.clientX - overviewDrag.x) * overviewView.width / bounds.width;
    overviewView.y -= (event.clientY - overviewDrag.y) * overviewView.height / bounds.height;
    overviewDrag.x = event.clientX;
    overviewDrag.y = event.clientY;
    clampOverview();
  } else {
    chooseOverviewCenter(point);
  }
});
overview.addEventListener('pointerup', () => { overviewDrag = null; });
overview.addEventListener('pointercancel', () => { overviewDrag = null; });
overview.addEventListener('wheel', event => {
  event.preventDefault();
  const point = overviewPoint(event);
  const factor = event.deltaY < 0 ? 0.8 : 1.25;
  const width = Math.min(360, Math.max(18, overviewView.width * factor));
  const height = width / 2;
  overviewView.x = point.x - (point.x - overviewView.x) * width / overviewView.width;
  overviewView.y = point.y - (point.y - overviewView.y) * height / overviewView.height;
  overviewView.width = width;
  overviewView.height = height;
  clampOverview();
}, { passive: false });
overview.addEventListener('dblclick', () => {
  Object.assign(overviewView, { x: 0, y: 0, width: 360, height: 180 });
  clampOverview();
});
clampOverview();
selectRegion();
const readJson = async response => {
  if (!response.ok) throw new Error(await response.text());
  return response.json();
};
const refreshPackages = async () => {
  const packages = await readJson(await fetch('/maps'));
  packageSelect.replaceChildren(new Option(packages.length ? 'Select a saved map' : 'No saved maps', ''));
  for (const item of packages) {
    const latitude = (item.request.center_latitude_e7 / 1e7).toFixed(3);
    const longitude = (item.request.center_longitude_e7 / 1e7).toFixed(3);
    const detail = item.uses_fallback_data ? 'fallback' : `${item.source_lock_count} sources`;
    packageSelect.add(new Option(`${latitude}, ${longitude} · ${item.estimate.tiles_per_side.toLocaleString()} tiles · ${detail}`, item.content_hash));
  }
};
const preparationSummary = plan => {
  const grid = plan.samples_per_axis ? `${plan.samples_per_axis} × ${plan.samples_per_axis} samples · ${(plan.geographic_millimeters_per_sample / 1000).toLocaleString()} m spacing\n` : '';
  return grid + plan.explanation;
};
const sequence = initStartSequence({ mapRequest, readJson, start: request => jobs.start(request), cancel: () => jobs.cancel() });
const jobs = initMapJobs({ readJson, controllerHeaders, preparationSummary, mapRequest, refreshPackages, estimate, generate, retry, cancel,
  onStart: sequence.begin, onProgress: sequence.progress, onActivated: sequence.activated, onError: sequence.error, onSettled: sequence.settled,
  onCancelable: sequence.cancelable, onCancelled: sequence.cancelled });
const { refreshHistory } = jobs;
sequence.recover(jobs.pending());
document.getElementById('creator-toggle').addEventListener('click', () => {
  document.getElementById('game-menu').close();
  creator.hidden = !creator.hidden;
  if (!creator.hidden) Promise.all([refreshPackages(), refreshHistory()]).catch(error => {
    estimate.className = 'error';
    estimate.textContent = error.message || 'Unable to list saved maps.';
  });
});
document.getElementById('refresh-maps').addEventListener('click', () => {
  Promise.all([refreshPackages(), refreshHistory()]).catch(error => {
    estimate.className = 'error';
    estimate.textContent = error.message || 'Unable to list saved maps.';
  });
});
document.getElementById('preview-map').addEventListener('click', async () => {
  if (!packageSelect.value) return;
  try {
    currentTerrainPreview = await readJson(await fetch(`/maps/${encodeURIComponent(packageSelect.value)}/preview`));
    drawTerrainPreview(currentTerrainPreview);
  } catch (error) {
    estimate.className = 'error';
    estimate.textContent = error.message || 'Unable to preview this saved map.';
  }
});
previewLayer.addEventListener('change', () => drawTerrainPreview(currentTerrainPreview));
document.getElementById('estimate-map').addEventListener('click', async () => {
  try {
    const request = mapRequest();
    const value = await readJson(await fetch('/maps/estimate', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(request) }));
    estimate.className = '';
    estimate.textContent = `${value.tiles_per_side.toLocaleString()} × ${value.tiles_per_side.toLocaleString()} tiles\nCompression: ${formatCompression(request.compression)} · Effective square: ${(value.effective_side_meters / 1000).toLocaleString()} km\nTile detail: ${(value.geographic_millimeters_per_tile / 1000).toLocaleString()} m\nStraight crossing: walk ${formatTime(value.walking_crossing_seconds)}, cavalry ${formatTime(value.cavalry_crossing_seconds)}\n${preparationSummary(value.preparation)}`;
  } catch (error) {
    estimate.className = 'error';
    estimate.textContent = error.message || 'Unable to estimate this map.';
  }
});
document.getElementById('open-map').addEventListener('click', async () => {
  if (!packageSelect.value) return;
  try {
    const value = await readJson(await fetch(`/maps/${encodeURIComponent(packageSelect.value)}`, { method: 'POST', headers: controllerHeaders() }));
    estimate.className = '';
    if (!value.start_available) {
      estimate.textContent = `Saved map available for preview: ${value.message || 'No suitable land start.'} Gameplay was not replaced.`;
      return;
    }
    estimate.textContent = `Saved map active: ${value.content_hash.slice(0, 12)}… Reconnect to join the activated world.`;
  } catch (error) {
    estimate.className = 'error';
    estimate.textContent = error.message || 'Unable to open this map.';
  }
});
init().catch(error => {
  document.getElementById('connection').textContent = 'Unavailable';
  document.getElementById('unsupported').textContent = `Unable to start AoeWorld: ${error}`;
  sequence.error('The game could not connect. Reload the page to try again.');
});
