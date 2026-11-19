const FOREST = 96, CADENCE_MS = 250;

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
    const label = String(count);
    context.font = 'bold 16px Georgia,serif';
    const badge = Math.max(26, context.measureText(label).width + 12);
    context.beginPath(); context.roundRect(-badge / 2, 8, badge, 22, 6);
    context.fillStyle = '#18252a'; context.fill();
    context.fillStyle = '#fff6d8';
    context.textAlign = 'center'; context.textBaseline = 'middle';
    context.fillText(label, 0, 19);
  }
  context.restore();
}

export function initResourceMarkers() {
  let raster = null, source, view, stamp = -Infinity;
  let grounds = [], forests = [], deposits = [];
  return {
    terrain(value) { raster = value; },
    ready(nextSource, nextView, now) {
      return nextSource !== source || (nextView !== view && now - stamp >= CADENCE_MS);
    },
    update(state, scale, project, fog, resources, nextSource, nextView, now) {
      const [worldWidth, worldHeight, cx, cy, width, height, zoom] = state;
      source = nextSource; view = nextView; stamp = now;
      deposits = [];
      // Cover at most three columns by two rows, with a 24-tile minimum span.
      const groundSize = Math.max(width / 3, height / 2, 24 * 64 * zoom) * scale;
      const water = patches(groundSize, width * scale), wood = patches(FOREST, width * scale);
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
      // A ground that straddles a coast keeps its marker over known fish.
      const terrainAt = (x, y) => {
        const { samples, cells } = raster;
        return cells[Math.floor(y * samples) * samples + Math.floor(x * samples)] & 15;
      };
      grounds = water.values(([x, y]) => {
        if (!raster) return true;
        const u = x / worldWidth, v = y / worldHeight;
        const kind = terrainAt(u, v);
        return kind >= 8 && kind <= 11 && (fog.disabled || fog.explored(u, v));
      });
      forests = [];
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
          return terrainAt(u, v) === 2 && (fog.disabled || fog.explored(u, v));
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
    drawForests(context, project, scale, width, height) {
      let count = 0;
      for (const patch of forests) {
        const [x, y] = project(patch.world);
        if (x >= 0 && x < width && y >= 0 && y < height) {
          icon(context, [x, y], patch.count, scale, true);
          count++;
        }
      }
      return count;
    },
    drawFishingGrounds(context, project, scale, width, height, space) {
      let count = 0;
      const offsets = [[0, 0], [0, 44], [44, 0], [-44, 0], [0, -44], [44, 44], [-44, 44], [44, -44], [-44, -44]];
      for (const patch of grounds) {
        const [x, y] = project(patch.world);
        if (!(x >= 0 && x < width && y >= 0 && y < height)) continue;
        const px = x * scale, py = y * scale;
        const chosen = offsets.find(([dx, dy]) => {
          const sx = px + dx, sy = py + dy;
          const rect = [sx - 28, sy - 9, sx + 28, sy + 32];
          if (rect[0] < 0 || rect[1] < 0 || rect[2] > width * scale || rect[3] > height * scale
            || !space.free(...rect)) return false;
          space.take(...rect, -4);
          return true;
        });
        const [dx, dy] = chosen ?? offsets[0];
        icon(context, [(px + dx) / scale, (py + dy) / scale], patch.count, scale, false);
        count++;
      }
      return count;
    },
  };
}
