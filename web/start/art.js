// Local generated paintings (never in Git): `make assets-series-art` writes JPEG
// copies and manifest.json to ignored local-assets/series-art/, which the dev
// server publishes at /series-art/. Without them every caller falls back to the
// bundled art and nothing else changes.
const BASE = '/series-art/';
const SAFE = /^[\w.-]+\/[\w.-]+\.jpg$/;
let manifest;

function valid(value) {
  if (value?.version !== 1 || !Array.isArray(value.series)) return null;
  const series = value.series
    .filter(entry => typeof entry?.id === 'string' && Array.isArray(entry.images))
    .map(entry => ({ id: entry.id, name: String(entry.name ?? entry.id),
      images: entry.images.filter(image => SAFE.test(image?.file ?? '')).map(image => ({
        url: BASE + image.file, culture: String(image.culture ?? '') })) }))
    .filter(entry => entry.images.length > 0);
  return series.length ? series : null;
}

/** All series, or null when the local copies are absent or invalid. */
export function seriesArt() {
  manifest ??= fetch(`${BASE}manifest.json`)
    .then(response => (response.ok ? response.json() : null)).then(valid).catch(() => null);
  return manifest;
}

export const pick = list => list[Math.floor(Math.random() * list.length)];
export function shuffled(list) {
  const copy = [...list];
  for (let at = copy.length - 1; at > 0; at--) {
    const other = Math.floor(Math.random() * (at + 1));
    [copy[at], copy[other]] = [copy[other], copy[at]];
  }
  return copy;
}

/** Resolves once the browser can paint the image; rejects if it cannot load. */
export function preload(url) {
  const image = new Image();
  image.src = url;
  return image.decode().then(() => url);
}

/** A random "kings with globes" painting for the menu, or null. */
export async function menuImage() {
  const kings = (await seriesArt())?.find(entry => entry.id === '01-kings-with-globes');
  if (!kings) return null;
  const image = pick(kings.images);
  try { return await preload(image.url); } catch { return null; }
}
