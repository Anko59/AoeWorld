// Sightings come exclusively from the authorized stream. Missing contacts
// become historical markers, never extrapolated enemy positions or orders.
export function initContacts() {
  const history = new Map();
  let current = new Set();
  return {
    reset() { history.clear(); current.clear(); },
    clock() { return history.size > current.size ? Math.floor(performance.now() / 1000) : 0; },
    update(values = []) {
      current = new Set();
      const now = performance.now();
      for (let i = 0; i + 2 < values.length && i < 3072; i += 3) {
        const id = values[i], world = [values[i + 1], values[i + 2]];
        if (!Number.isFinite(id)) continue;
        // A currently visible corpse explicitly retires its living sighting.
        if (!world.every(Number.isFinite)) { history.delete(id); continue; }
        current.add(id);
        history.delete(id);
        history.set(id, { world, time: now });
      }
      for (const [id, contact] of history) {
        if (now - contact.time > 120000 || history.size > 1024) history.delete(id);
      }
    },
    draw(context, project) {
      let count = 0;
      const now = performance.now();
      context.strokeStyle = '#f09880';
      context.fillStyle = '#f5b5a1';
      context.font = '11px sans-serif';
      context.textAlign = 'left';
      for (const [id, contact] of history) {
        if (current.has(id) || now - contact.time > 120000) continue;
        const [x, y] = project(contact.world);
        context.setLineDash([3, 3]);
        context.strokeRect(x - 6, y - 6, 12, 12);
        context.setLineDash([]);
        context.fillText(`seen ${Math.floor((now - contact.time) / 1000)}s ago`, x + 9, y);
        count++;
      }
      return count;
    },
  };
}
