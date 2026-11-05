import { icon } from '/interface/ui-art.js';
import { buildingNames } from '/interface/buildings.js';
import { iconGrid } from './grid.js';

// Research from the far view. Rust reports, for every technology, an availability
// code and the owned building that would research it; this panel only shows it.
const reasons = ['', 'Ready', 'Feudal Age', 'Done', 'In progress', 'Training', 'Food', 'Wood', 'Gold', 'Stone', 'Queue full', 'Building', 'Needs buildings', '', 'Castle Age', '', 'Imperial Age'];
export function initResearch(host) {
  const root = document.createElement('div');
  root.innerHTML = '<p class="far-hint" data-empty>Research becomes available here as soon as you own the building that teaches it.</p><p class="far-status" role="status"></p>';
  host.append(root);
  const status = root.querySelector('[role=status]'), empty = root.querySelector('[data-empty]');
  let groups = null, report = [];
  function build() {
    const techs = window.aoeBuildings?.techs;
    if (groups || !techs?.length) return;
    groups = [];
    const sites = [...new Set(techs.map(t => t.site))].sort((a, b) => a - b);
    for (const site of sites) {
      const title = document.createElement('div'); title.className = 'far-group'; title.textContent = buildingNames[site] ?? 'Building';
      const members = techs.filter(t => t.site === site);
      const grid = iconGrid(`${buildingNames[site]} research`, members.map(t => ({
        id: t.index, label: t.label.split(' · ')[0], art: icon('technologies', t.art, 'technology'),
      })), index => {
        const bid = report[4 + index * 2 + 1];
        if (bid >= 0) {
          window.aoeBuildings?.command(12, index, bid);
          status.textContent = `Ordered ${members.find(t => t.index === index).label.split(' · ')[0]} at ${buildingNames[site]} #${bid}.`;
        }
      });
      for (const t of members) grid.set(t.index, { title: t.label });
      root.insertBefore(title, status); root.insertBefore(grid.grid, status);
      groups.push({ title, grid, members });
    }
  }
  return {
    update(values) {
      report = Array.from(values); build();
      if (!groups) return;
      let any = false;
      for (const g of groups) {
        let shown = 0;
        for (const t of g.members) {
          const code = report[4 + t.index * 2], at = report[4 + t.index * 2 + 1];
          const visible = Number.isFinite(code) && at >= 0 && code !== 0 && code !== 3;
          g.grid.set(t.index, { hidden: !visible, enabled: code === 1, note: visible ? (code === 1 ? t.label.split(' · ').slice(1).join(' · ').replace(/ (food|wood|gold|stone)/g, m => m[1].toUpperCase()) : reasons[code] ?? '') : '',
            title: `${t.label}${code === 1 ? '' : ` · ${reasons[code] ?? ''}`}` });
          if (visible) shown++;
        }
        g.title.hidden = g.grid.grid.hidden = shown === 0; any ||= shown > 0;
      }
      empty.hidden = any;
    },
  };
}
