const minuteMs = 60_000;

// Divide the interval at span boundaries so a GL call or collection inside a
// renderer span is counted once. Spans are in increasing priority order.
const priority = [
  "browser_main",
  "render",
  "update",
  "interpolation",
  "culling",
  "ui",
  "terrain",
  "sprites_and_submission",
  "fog_and_packets",
  "socket_decode_apply",
  "snapshot",
  "buffer_upload",
  "texture_upload",
  "webgl_validation",
  "shader_compile",
  "gc",
];

function exclusive({ ms, spans }) {
  const clipped = spans
    .map(([a, b, kind]) => [Math.max(0, a), Math.min(ms, b), kind])
    .filter(([a, b, kind]) => a < b && priority.includes(kind));
  const boundaries = [
    ...new Set([0, ms, ...clipped.flatMap(([a, b]) => [a, b])]),
  ].sort((a, b) => a - b);
  const totals = {};
  for (let index = 1; index < boundaries.length; index++) {
    const a = boundaries[index - 1],
      b = boundaries[index];
    const kind = clipped.reduce(
      (best, [start, end, name]) =>
        start <= a &&
        end >= b &&
        priority.indexOf(name) > priority.indexOf(best)
          ? name
          : best,
      "unattributed",
    );
    totals[kind] = (totals[kind] ?? 0) + b - a;
  }
  return totals;
}

export function summarize(frames) {
  if (
    !frames.length ||
    frames.some(({ ms }) => !Number.isFinite(ms) || ms <= 0)
  ) {
    throw new Error("missing or invalid frame intervals");
  }
  const sorted = frames.map(({ ms }) => ms).sort((a, b) => a - b);
  const elapsed_ms = sorted.reduce((sum, ms) => sum + ms, 0);
  const percentile = (p) => sorted[Math.ceil(p * sorted.length) - 1];
  const stalls = frames.flatMap((frame, index) => {
    if (frame.ms <= 50) return [];
    const exclusive_ms = exclusive(frame);
    const cause = Object.keys(exclusive_ms).reduce(
      (largest, name) =>
        exclusive_ms[name] > (exclusive_ms[largest] ?? 0) ? name : largest,
      "unattributed",
    );
    return [{ index, ms: frame.ms, cause, exclusive_ms }];
  });
  function threshold(ms) {
    const selected = stalls.filter((frame) => frame.ms > ms);
    const causes = {};
    for (const { cause } of selected) causes[cause] = (causes[cause] ?? 0) + 1;
    return {
      count: selected.length,
      per_minute: (selected.length * minuteMs) / elapsed_ms,
      causes,
    };
  }
  return {
    elapsed_ms,
    frames: frames.length,
    p50_ms: percentile(0.5),
    p99_ms: percentile(0.99),
    max_ms: sorted.at(-1),
    over_50: threshold(50),
    over_100: threshold(100),
    stalls,
  };
}

// Approximate the isometric viewport on the ground when choosing a camera.
// The benchmark separately verifies the actual rendered actors at that pose.
export function selectCrowd(entities, [width, height]) {
  const towns = entities.filter((e) => e.kind === "TownCenter");
  let best;
  for (const {
    position: { x, y },
  } of towns) {
    const candidate = { x, y, units: 0, attackers: 0, buildings: 0 };
    for (const { position, category, task } of entities) {
      const dx = position.x - x,
        dy = position.y - y;
      if (Math.abs(dx - dy) > width || Math.abs(dx + dy) > height) continue;
      if (category === "building") candidate.buildings++;
      else {
        candidate.units++;
        candidate.attackers += Number(task === "Attacking");
      }
    }
    if (
      candidate.attackers > 0 &&
      (!best ||
        candidate.units > best.units ||
        (candidate.units === best.units &&
          candidate.attackers > best.attackers))
    )
      best = candidate;
  }
  if (!best)
    throw new Error(
      "save contains no town with combat in the detailed viewport",
    );
  return best;
}
