// Fish nodes have no shoal IDs. Group nearby known nodes at a fixed screen
// size, so the kingdom view also has a bounded number of resource symbols.
const SHOAL = 68, FOREST = 96, CADENCE_MS = 250;

function patches(size, width) {
  const bins = new Map();
  const stride = Math.ceil(width / size) + 2;
  return {
    add(x, y, world) {
      const col = Math.floor(x / size), row = Math.floor(y / size);
      const key = (row + 1) * stride + col + 1;
      let patch = bins.get(key), best = size * size;
      if (!patch) {
        for (let dy = -1; dy <= 1; dy++) for (let dx = -1; dx <= 1; dx++) {
          const near = bins.get(key + dy * stride + dx);
          if (!near) continue;
          const distance = (near.x - x) ** 2 + (near.y - y) ** 2;
          if (distance < best) { patch = near; best = distance; }
        }
      }
      if (patch) {
        patch.count++;
        patch.sumX += world[0]; patch.sumY += world[1];
        return;
      }
      bins.set(key, { x, y, world, count: 1, sumX: world[0], sumY: world[1] });
    },
    values(useCenter = () => true) {
      return [...bins.values()].map(({ count, world, sumX, sumY }) => {
        const center = [sumX / count, sumY / count];
        return { count, world: useCenter(center) ? center : world };
      });
    },
  };
}

const fish = new Path2D('M-9 0Q-2-8 5-2L10-6V6L5 2Q-2 8-9 0Z');
const trees = new Path2D('M-11 5l4-8 4 8Zm7 0 5-13 5 13Zm8 0 4-9 4 9ZM0 5h2v3H0Z');

function icon(context, [x, y], count, scale, forest) {
  context.save();
  context.translate(x, y);
  const size = forest ? Math.min(1.4, 1 + Math.log2(count) / 12) : 1;
  context.scale(size / scale, size / scale);
  context.lineJoin = 'round';
  context.strokeStyle = '#18252a'; context.lineWidth = 3;
  context.fillStyle = forest ? '#b7d389' : '#a0def6';
  const shape = forest ? trees : fish;
  context.stroke(shape); context.fill(shape);
  if (!forest) {
    context.fillStyle = '#fff6d8';
    context.font = 'bold 12px Georgia,serif';
    context.textAlign = 'center'; context.textBaseline = 'top';
    context.strokeText(String(count), 0, 7);
    context.fillText(String(count), 0, 7);
  }
  context.restore();
}

export function initResourceMarkers() {
  let raster = null, source, view, stamp = -Infinity;
  let shoals = [], forests = [], deposits = [];
  return {
    terrain(value) { raster = value; },
    ready(nextSource, nextView, now) {
      return nextSource !== source || (nextView !== view && now - stamp >= CADENCE_MS);
    },
    update(state, scale, project, fog, resources, nextSource, nextView, now) {
      const [worldWidth, worldHeight, cx, cy, width, height, zoom] = state;
      source = nextSource; view = nextView; stamp = now;
      deposits = [];
      const water = patches(SHOAL, width * scale), wood = patches(FOREST, width * scale);
      const inView = (x, y) => x >= 0 && x < width && y >= 0 && y < height;
      const nodes = resources.filter(node => node.kind >= 4)
        .sort((a, b) => a.world[0] - b.world[0] || a.world[1] - b.world[1]);
      for (const node of nodes) {
        const [x, y] = project(node.world);
        if (inView(x, y)) water.add(x * scale, y * scale, node.world);
      }
      for (const node of resources) {
        if (node.kind >= 4) continue;
        const [x, y] = project(node.world);
        if (inView(x, y)) deposits.push(node);
      }
      shoals = water.values(); forests = [];
      if (raster) {
        // Invert the isometric view bounds before walking the forest raster.
        const { samples, cells } = raster;
        const reach = width / (256 * zoom) + height / (128 * zoom);
        const dx = worldWidth / samples, dy = worldHeight / samples;
        const right = Math.min(samples, Math.ceil((cx + reach) / dx));
        const bottom = Math.min(samples, Math.ceil((cy + reach) / dy));
        for (let row = Math.max(0, Math.floor((cy - reach) / dy)); row < bottom; row++) {
          for (let col = Math.max(0, Math.floor((cx - reach) / dx)); col < right; col++) {
            if ((cells[row * samples + col] & 15) !== 2) continue;
            const u = (col + .5) / samples, v = (row + .5) / samples;
            if (!fog.disabled && !fog.explored(u, v)) continue;
            const world = [u * worldWidth, v * worldHeight];
            const [x, y] = project(world);
            if (inView(x, y)) wood.add(x * scale, y * scale, world);
          }
        }
        forests = wood.values(([x, y]) => {
          const u = x / worldWidth, v = y / worldHeight;
          const at = Math.floor(v * samples) * samples + Math.floor(u * samples);
          return (cells[at] & 15) === 2 && (fog.disabled || fog.explored(u, v));
        });
      }
    },
    drawDeposits(context, project, level) {
      const colours = ['#e85b8a', '#3c8f4a', '#f2cf50', '#c4c8d0'];
      const radius = level === 2 ? 3 : 2;
      for (const { world, kind } of deposits) {
        const [x, y] = project(world);
        context.fillStyle = '#101910';
        context.fillRect(x - radius - 1, y - radius - 1, radius * 2 + 2, radius * 2 + 2);
        context.fillStyle = colours[kind];
        context.fillRect(x - radius, y - radius, radius * 2, radius * 2);
      }
    },
    draw(context, project, scale, width, height) {
      const inView = ([x, y]) => x >= 0 && x < width && y >= 0 && y < height;
      let forestCount = 0, shoalCount = 0;
      for (const patch of forests) {
        const point = project(patch.world);
        if (inView(point)) { icon(context, point, patch.count, scale, true); forestCount++; }
      }
      for (const patch of shoals) {
        const point = project(patch.world);
        if (inView(point)) { icon(context, point, patch.count, scale, false); shoalCount++; }
      }
      return [shoalCount, forestCount];
    },
  };
}
