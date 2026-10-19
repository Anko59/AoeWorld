// Where a resource node is on the canvas, and whether it can be clicked. The
// isometric projection (64 x 32 px per tile at zoom 1) is the client's.

// The HUD's bands (browser/tests/village/interface.play.spec.ts): the top
// bar and the bottom panel take no clicks meant for the map.
export const HUD_TOP = 54;
export const HUD_BOTTOM = 208;
const EDGE = 24;

// The world tile under a canvas point, for the camera it was read under.
export function worldAt(camera, view, point) {
  const dx = (point.x - view.width / 2) / (64 * camera.zoom);
  const dy = (point.y - view.height / 2) / (32 * camera.zoom);
  return { x: camera.x + (dx + dy) / 2, y: camera.y + (dy - dx) / 2 };
}

// The camera that puts `world` in the middle of the canvas at `zoom`: the
// middle is clear of the HUD whatever the viewport.
export function centredOn(world, zoom) {
  return { x: world.x, y: world.y, zoom };
}

// Can a click at this canvas point reach the map, clear of the HUD and edges?
export function clickable(view, point) {
  return point.x >= EDGE && point.x <= view.width - EDGE
    && point.y >= HUD_TOP + EDGE && point.y <= view.height - HUD_BOTTOM - EDGE;
}

// The nearest node of `kind` the client lists, or undefined. The client lists
// one node per kind, the one nearest the selected villager.
export function nearestOf(targets, kind) {
  return targets.filter((r) => r.kind === kind).sort((a, b) => a.distance - b.distance)[0];
}

// Two reads of a node agree: it has stopped moving across the screen.
export function steady(a, b) {
  return Boolean(a) && Boolean(b) && Math.abs(a.x - b.x) < 0.5 && Math.abs(a.y - b.y) < 0.5;
}

// A clamped camera is allowed to stop short of its requested aim.
export function sameCamera(a, b) {
  return Boolean(a) && Boolean(b) && Math.abs(a.x - b.x) <= 0.01
    && Math.abs(a.y - b.y) <= 0.01 && Math.abs(a.zoom - b.zoom) <= 0.001;
}
