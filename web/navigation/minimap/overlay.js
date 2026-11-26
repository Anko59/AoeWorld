// Symbols at world points of the minimap, over its marks and its frame. Their
// unit is one page px of the drawn map, half as much again on the whole map, so
// neither the stretch nor the zoom of the minimap reaches them.
import { minimapProject } from '/minimap.js';

const LARGE = 400;     // the whole-map overview

const f = value => String(Math.round(value * 100) / 100);

function attribute(node, name, value) {
  if (node.getAttribute(name) !== value) node.setAttribute(name, value);
}

export function initOverlay(layer) {
  const symbols = new Map();
  let seat = null;

  // A symbol beyond the drawn map is kept: the map may come back to it.
  function put(node, symbol) {
    symbol.shown = false;
    if (seat) {
      const { world, view, width, height } = seat, [x, y] = minimapProject(symbol.point, world, view);
      symbol.x = x * width / 256;
      symbol.y = y * height / 256;
      symbol.shown = symbol.x >= 0 && symbol.y >= 0 && symbol.x <= width && symbol.y <= height;
      const large = Math.min(width, height) >= LARGE ? ' scale(1.5)' : '';
      attribute(node, 'transform', `translate(${f(symbol.x)} ${f(symbol.y)})${large}`);
    }
    attribute(node, 'display', symbol.shown ? 'inline' : 'none');
  }
  return {
    add(node, point) {
      const symbol = { point, x: 0, y: 0, shown: false };
      symbols.set(node, symbol);
      put(node, symbol);
      layer.append(node);
    },
    remove(node) {
      symbols.delete(node);
      node.remove();
    },
    // The world point of the symbol nearest to a px of the drawn map.
    nearest(x, y, reach) {
      let found = null;
      for (const symbol of symbols.values()) {
        const apart = Math.hypot(symbol.x - x, symbol.y - y);
        if (symbol.shown && apart <= reach) { found = symbol.point; reach = apart; }
      }
      return found;
    },
    // Called when the map was measured, moved or zoomed, not for each frame.
    draw(world, view, frame) {
      seat = frame && { world, view, width: frame.width, height: frame.height };
      if (seat) attribute(layer, 'transform', `scale(${256 / seat.width} ${256 / seat.height})`);
      for (const [node, symbol] of symbols) put(node, symbol);
    },
  };
}
