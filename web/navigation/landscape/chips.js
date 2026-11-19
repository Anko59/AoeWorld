import { drawMiniature } from '/navigation/landscape/units.js';
import { actorColour } from '/minimap.js';

// Small plaques count buildings and the unit groups folded into each place.
export function drawChips(context, placed, scale, artFor) {
  const u = 1 / scale;
  context.font = `bold ${Math.round(11 * u)}px Georgia,serif`;
  context.textBaseline = 'middle'; context.textAlign = 'left';
  for (const p of placed) {
    const items = [];
    if (p.buildings > 1) items.push({ count: p.buildings });
    for (const [type, group] of p.units) items.push({ type, ...group });
    if (!items.length) continue;
    const cell = 18 * u, gap = 5 * u;
    const texts = items.map(({ count }) => String(count >= 10000 ? `${Math.round(count / 1000)}k` : count));
    const widths = texts.map(t => cell + 2 * u + context.measureText(t).width);
    const total = widths.reduce((a, b) => a + b, 0) + gap * (items.length - 1) + 8 * u;
    let x = p.x - total / 2;
    const y = p.top + p.h + 6 * u;
    context.beginPath(); context.roundRect(x, y - 10 * u, total, 20 * u, 8 * u);
    context.fillStyle = p.owned ? '#1b140ccc' : '#1b140c99'; context.fill();
    x += 4 * u;
    items.forEach(({ type, kind }, i) => {
      const cx = x + cell / 2;
      if (type !== undefined) drawMiniature(context, cx, y, 16 * u, type, actorColour(p.colour), artFor(kind));
      else { context.fillStyle = '#d8c594'; context.beginPath(); context.moveTo(cx - 5 * u, y); context.lineTo(cx, y - 5 * u); context.lineTo(cx + 5 * u, y); context.lineTo(cx + 4 * u, y + 5 * u); context.lineTo(cx - 4 * u, y + 5 * u); context.closePath(); context.fill(); }
      context.fillStyle = '#fff6d8'; context.fillText(texts[i], x + cell + 2 * u, y + .5 * u);
      x += widths[i] + gap;
    });
  }
}
