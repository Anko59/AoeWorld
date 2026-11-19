// The actor stream is sampled at HUD cadence. Camera animation reprojects the
// previous group centroids between samples, including a margin for panning.
const CADENCE_MS = 250;
export const CELL = 44;

export function tally(group, kind, count = 1) {
  const total = (group.kinds.get(kind) ?? 0) + count;
  group.kinds.set(kind, total);
  if (total > group.dominantCount || (total === group.dominantCount && kind < group.kind)) {
    group.kind = kind;
    group.dominantCount = total;
  }
  group.count += count;
}

export function emptyGroup() {
  return { kinds: new Map(), count: 0, dominantCount: 0, kind: -1 };
}

function aggregate(records, project, width, height, scale) {
  const bins = new Map();
  const margin = CELL * 2;
  const rows = Math.ceil(height * scale / CELL) + 4;
  for (let at = 0; at + 4 < records.length; at += 5) {
    const tag = records[at];
    if (tag & 1) continue;
    const x = records[at + 1], y = records[at + 2];
    const [sx, sy] = project([x, y]);
    const px = sx * scale, py = sy * scale;
    if (px < -margin || px >= width * scale + margin || py < -margin || py >= height * scale + margin) continue;
    const cellX = Math.floor(px / CELL), cellY = Math.floor(py / CELL);
    const owner = records[at + 3], type = tag & 6;
    // Each cell reserves all four classes for every u16 player id.
    const cell = (cellX + 2) * rows + cellY + 2;
    const key = (cell * 65536 + owner) * 4 + (type >> 1);
    let group = bins.get(key);
    if (!group) {
      group = { ...emptyGroup(), type, owner, friendly: !!(tag & 8), colour: records[at + 4], world: [0, 0] };
      bins.set(key, group);
    }
    tally(group, (tag >> 4) - 1);
    group.world[0] += x; group.world[1] += y;
  }
  return [...bins.values()].sort((a, b) => a.friendly - b.friendly || b.count - a.count
    || a.world[0] / a.count - b.world[0] / b.count || a.world[1] / a.count - b.world[1] / b.count
    || a.owner - b.owner || a.type - b.type).map(group => {
    group.world[0] /= group.count; group.world[1] /= group.count;
    return group;
  });
}

export function initUnitGroups() {
  let revision, view, stamp = -Infinity;
  let groups = [];
  return {
    ready(nextRevision, nextView, now) {
      return (nextRevision !== revision || nextView !== view) && now - stamp >= CADENCE_MS;
    },
    update(records, nextRevision, nextView, now, project, width, height, scale) {
      groups = aggregate(records, project, width, height, scale);
      revision = nextRevision; view = nextView; stamp = now;
    },
    groups: () => groups,
    reset() { revision = view = undefined; stamp = -Infinity; groups = []; },
  };
}
