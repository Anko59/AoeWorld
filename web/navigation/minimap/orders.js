// Orders pressed on the strategic map. The client decides what a press means;
// the map only tells it the place and marks what was sent.
import { minimapProject } from '/minimap.js';

// The client answers 0 for a press that is no order, 1 to show nothing, and
// these from 2 on.
const PINGS = [['move', '#7ee081'], ['attack-move', '#e0463a'], ['patrol', '#f2cf50']];
const MAX_PINGS = 8;

export function initMinimapOrders(svg) {
  let send = null;
  // Last in the map: over the marks, which would hide most of a ping.
  const layer = document.createElementNS(svg.namespaceURI, 'g');
  layer.id = 'minimap-pings';
  svg.append(layer);
  // Zero-length round strokes keep their size at any zoom or stretch, and the
  // land's transform keeps them in place. Stop removes pings from the page.
  function ping([kind, colour], [x, y]) {
    const node = document.createElementNS(svg.namespaceURI, 'g');
    node.setAttribute('data-minimap-ping', kind);
    node.setAttribute('pointer-events', 'none');
    node.innerHTML = [[15, '#17251b'], [12, colour], [5, '#17251b']].map(([width, stroke]) =>
      `<path d="M${x} ${y}h0" stroke="${stroke}" stroke-width="${width}" stroke-linecap="round" vector-effect="non-scaling-stroke"/>`).join('');
    layer.append(node);
    if (layer.children.length > MAX_PINGS) layer.firstElementChild.remove();
    setTimeout(() => node.remove(), 1500);
  }
  return {
    bind(callback) { send = callback; },
    draw() { layer.setAttribute('transform', document.getElementById('minimap-view').getAttribute('transform')); },
    // True when the press was an order: it starts no drag and no jump.
    press(point, world, event) {
      const [x, y] = point ?? [NaN, NaN];
      const answer = send?.(x, y, event.button === 0, event.shiftKey || event.ctrlKey, event.altKey) ?? 0;
      if (answer > 1) ping(PINGS[answer - 2], minimapProject(point, world));
      return answer > 0;
    },
  };
}
