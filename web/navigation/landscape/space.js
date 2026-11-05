// One screen-space occupancy grid (CSS pixels) shared by every far-view layer
// so symbols never overlap. Each cell holds the id of the piece that owns it;
// -1 is reserved HUD. Cost is proportional to the rectangle area, never to the
// number of units or buildings behind a symbol.
const CW = 10, CH = 8;
export function createSpace() {
  const cells = new Map();
  const key = (x, y) => (x + 1024) * 4096 + (y + 1024);
  const range = (x0, y0, x1, y1) => [Math.floor(x0 / CW), Math.floor(y0 / CH), Math.floor((x1 - 1e-6) / CW), Math.floor((y1 - 1e-6) / CH)];
  return {
    clear() { cells.clear(); },
    free(x0, y0, x1, y1) {
      const [a, b, c, d] = range(x0, y0, x1, y1);
      for (let x = a; x <= c; x++) for (let y = b; y <= d; y++) if (cells.has(key(x, y))) return false;
      return true;
    },
    // First piece found inside a rectangle, used to fold a displaced symbol into its neighbour.
    owner(x0, y0, x1, y1) {
      const [a, b, c, d] = range(x0, y0, x1, y1);
      for (let x = a; x <= c; x++) for (let y = b; y <= d; y++) { const id = cells.get(key(x, y)); if (id !== undefined) return id; }
      return undefined;
    },
    take(x0, y0, x1, y1, id) {
      const [a, b, c, d] = range(x0, y0, x1, y1);
      for (let x = a; x <= c; x++) for (let y = b; y <= d; y++) if (!cells.has(key(x, y))) cells.set(key(x, y), id);
    },
    at(x, y) { return cells.get(key(Math.floor(x / CW), Math.floor(y / CH))); },
  };
}
