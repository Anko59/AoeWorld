import { actorColour } from '/minimap.js';

// Build paths at HUD cadence from authorized row runs. Opposing edges of
// adjacent runs cancel, so the stroke follows holdings rather than tile rows.
export function initHoldings() {
  let previous = new Float64Array(), paths = new Map(), count = 0;
  return {
    update(values = []) {
      // Keep an owned numeric snapshot: a string signature boxes every value
      // even when the first changed row already tells us to rebuild the paths.
      let same = values.length === previous.length;
      for (let i = 0; same && i < values.length; i++) {
        const value = values[i], old = previous[i];
        same = value === old || Number.isNaN(value) && Number.isNaN(old);
      }
      if (same) return false;
      previous = new Float64Array(values); paths = new Map(); count = 0;
      const edges = new Map(), horizontal = new Map();
      function edge(colour, x0, y0, x1, y1) {
        const a = `${x0},${y0}`, b = `${x1},${y1}`;
        const key = `${colour}:${a < b ? `${a}:${b}` : `${b}:${a}`}`;
        if (edges.has(key)) edges.delete(key);
        else edges.set(key, [colour, x0, y0, x1, y1]);
      }
      for (let i = 0; i + 3 < values.length; i += 4) {
        const [x0, y, x1, colour] = values.slice(i, i + 4);
        if (![x0,y,x1,colour].every(Number.isFinite) || x1 <= x0) continue;
        const path = paths.get(colour) ?? { fill: new Path2D(), border: new Path2D() };
        path.fill.rect(x0, y, x1-x0, 1); paths.set(colour, path); count++;
        for (const [row, sign] of [[y,1],[y+1,-1]]) {
          const key = `${colour}:${row}`;
          const line = horizontal.get(key) ?? { colour, row, events: [] };
          line.events.push([x0,sign],[x1,-sign]); horizontal.set(key,line);
        }
        edge(colour,x0,y,x0,y+1); edge(colour,x1,y,x1,y+1);
      }
      for (const { colour, row, events } of horizontal.values()) {
        events.sort((a,b) => a[0]-b[0]);
        const border = paths.get(colour).border;
        let balance = 0, previous = 0;
        for (const [x, delta] of events) {
          if (balance !== 0 && x > previous) { border.moveTo(previous,row); border.lineTo(x,row); }
          balance += delta; previous = x;
        }
      }
      for (const [colour,x0,y0,x1,y1] of edges.values()) {
        const border = paths.get(colour).border;
        border.moveTo(x0,y0); border.lineTo(x1,y1);
      }
      return true;
    },
    draw(context, plane, zoom, enabled) {
      if (!enabled) return 0;
      plane(); context.lineWidth = 1.5/(64*zoom);
      for (const [colour, path] of paths) {
        context.fillStyle = context.strokeStyle = actorColour(colour);
        context.globalAlpha = .14; context.fill(path.fill);
        context.globalAlpha = .9; context.stroke(path.border);
      }
      context.globalAlpha = 1;
      return count;
    },
    reset() { previous = new Float64Array(); paths.clear(); count = 0; },
  };
}
