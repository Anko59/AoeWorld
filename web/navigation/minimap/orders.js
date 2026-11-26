// Orders pressed on the strategic map. The client decides what a press means;
// the map only tells it the place and marks what was sent.
import { initOverlay } from '/navigation/minimap/overlay.js';

// Tips at the origin: one opens away from the place, the other points away.
const CHEVRON = 'M3.2-3.2L0 0L3.2 3.2', HEAD = 'M-4.5-4.5L0 0L-4.5 4.5';
// The client answers 0 for a press that is no order, 1 to show nothing, and
// these from 2 on. A kind has its own shape; its colour only repeats it.
const PINGS = [
  { kind: 'move', outline: CHEVRON, turns: [0, 90, 180, 270], from: 11, to: 5 },
  { kind: 'attack-move', outline: CHEVRON, turns: [45, 135, 225, 315], from: 11, to: 5 },
  { kind: 'patrol', outline: HEAD, turns: [0, 180], from: 5, to: 11 },
];
const MAX_PINGS = 8;
const MOTION_MS = 400, FADE_MS = 300, STILL_MS = 900;

export function initMinimapOrders(layer) {
  const overlay = initOverlay(layer);
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');
  let send = null;

  // Stop removes pings from the page (village/waypoints.rs).
  function ping({ kind, outline, turns, from, to }, point) {
    const node = document.createElementNS(layer.namespaceURI, 'g');
    node.setAttribute('data-minimap-ping', kind);
    node.innerHTML = turns.map(turn => `<g transform="rotate(${turn})"><g transform="translate(${to})">
      <path class="ping-ink" d="${outline}"/><path class="ping-core" d="${outline}"/></g></g>`).join('');
    overlay.add(node, point);
    if (layer.children.length > MAX_PINGS) overlay.remove(layer.firstElementChild);
    if (!reduced.matches) {
      for (const turn of node.children) {
        turn.firstElementChild.animate({ transform: [`translate(${from}px)`, `translate(${to}px)`] },
          { duration: MOTION_MS, easing: 'cubic-bezier(.165,.84,.44,1)' });
      }
      node.animate({ opacity: [1, 0] }, { delay: MOTION_MS, duration: FADE_MS, fill: 'forwards' });
    }
    setTimeout(() => overlay.remove(node), reduced.matches ? STILL_MS : MOTION_MS + FADE_MS);
  }
  return {
    bind(callback) { send = callback; },
    draw: overlay.draw,
    // True when the press was an order: it starts no drag and no jump.
    press(point, event) {
      const [x, y] = point ?? [NaN, NaN];
      const answer = send?.(x, y, event.button === 0, event.shiftKey || event.ctrlKey, event.altKey) ?? 0;
      if (answer > 1) ping(PINGS[answer - 2], point);
      return answer > 0;
    },
  };
}
