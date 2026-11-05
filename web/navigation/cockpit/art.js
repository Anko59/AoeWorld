import { artReady, panelCrop } from '/interface/ui-art.js';

// Panel chrome is cut from the pack's own interface frames and applied with
// border-image, so the iron frame keeps its corners while the parchment or
// stone centre stretches to cover any container size. Nothing is saved to disk.
const frames = {
  '--far-parchment': ['western-selection', [0, 50, 1250, 362]],
  '--far-stone': ['western-commands', [0, 50, 541, 362]],
  '--far-key': ['button', [0, 0, 80, 80]],
};
let applied = false;
export function applyFarArt() {
  if (applied) return;
  artReady.then(async () => {
    const root = document.documentElement.style;
    for (const [name, [role, region]] of Object.entries(frames)) {
      const data = panelCrop(role, region);
      if (!data) continue;
      // A short blob: URL, not the data URL itself: custom properties are
      // re-substituted on every style recalculation, and a 400 KB string made each one slow.
      const url = URL.createObjectURL(await (await fetch(data)).blob());
      root.setProperty(name, `url('${url}')`);
    }
    document.documentElement.dataset.farArt = 'original';
    applied = true;
  });
}
