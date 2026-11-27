import { attribute, set } from '/interface/dom/update.js';

export function initRelics() {
  const byId = id => document.getElementById(id);
  const status = byId('relic-status');
  const banner = byId('relic-victory');
  const minimap = byId('minimap');
  const layer = byId('minimap-relics');
  const drop = byId('drop-relic');
  const eject = byId('eject-relic');
  drop.addEventListener('click', () => window.aoeBuildings?.command?.(20, 0, 0, 1));
  eject.addEventListener('click', () => window.aoeBuildings?.command?.(20, 1, 0, 1));
  let previous = '';
  window.aoeRelics = { update(info, marks) {
    const [carrying, count, leader, time, viewer, dropAllowed, ejectAllowed] = info;
    set(drop, 'hidden', !dropAllowed);
    set(eject, 'hidden', !ejectAllowed);
    set(status, 'hidden', !carrying && count < 0);
    const noun = count === 1 ? 'relic' : 'relics';
    set(status, 'textContent', carrying ? 'Carrying a relic' : count >= 0 ? `${count} ${noun} stored` : '');
    set(banner, 'hidden', leader < 0);
    if (leader >= 0) {
      const clock = `${Math.floor(time / 60)}:${String(time % 60).padStart(2, '0')}`;
      const name = leader === viewer ? 'You' : `Player ${leader + 1}`;
      set(banner, 'textContent', time > 0 ? `${name} hold${leader === viewer ? '' : 's'} all relics · ${clock}`
        : `${name} won a relic victory`);
    }
    const width = Number(minimap.dataset.worldWidth);
    const height = Number(minimap.dataset.worldHeight);
    const key = marks.length && width > 0 && height > 0 ? `${width}x${height}/${marks.join(',')}` : '';
    if (key === previous) return;
    previous = key;
    layer.replaceChildren();
    if (!key) return;
    for (let i = 0; i + 1 < marks.length; i += 2) {
      const icon = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
      attribute(icon, 'cx', marks[i] / width * 256);
      attribute(icon, 'cy', marks[i + 1] / height * 256);
      attribute(icon, 'r', '2.1');
      attribute(icon, 'fill', '#fff2b5');
      attribute(icon, 'stroke', '#66390e');
      layer.append(icon);
    }
  }};
}
