import { drawShield, drawVillagers } from '/navigation/landscape/units.js';

// Small plaques under a place: buildings folded into it, villagers working
// there and soldiers stationed there. One plaque per place, never per unit.
export function drawChips(context, placed, scale) {
  const u = 1 / scale;
  context.font = `bold ${Math.round(11 * u)}px Georgia,serif`;
  context.textBaseline = 'middle'; context.textAlign = 'left';
  for (const p of placed) {
    const items = [];
    if (p.buildings > 1) items.push(['house', p.buildings]);
    if (p.workers > 0) items.push(['villager', p.workers]);
    if (p.soldiers > 0) items.push(['soldier', p.soldiers]);
    if (!items.length) continue;
    const cell = 12 * u, gap = 5 * u;
    const texts = items.map(([, n]) => String(n >= 10000 ? `${Math.round(n / 1000)}k` : n));
    const widths = texts.map((t, i) => cell + 2 * u + context.measureText(t).width);
    const total = widths.reduce((a, b) => a + b, 0) + gap * (items.length - 1) + 8 * u;
    let x = p.x - total / 2;
    const y = p.top + p.h + 6 * u;
    context.beginPath(); context.roundRect(x, y - 8 * u, total, 16 * u, 8 * u);
    context.fillStyle = p.owned ? '#1b140ccc' : '#1b140c99'; context.fill();
    x += 4 * u;
    items.forEach(([kind], i) => {
      const cx = x + cell / 2;
      if (kind === 'soldier') drawShield(context, cx, y, 12 * u, '#c9b88a', true);
      else if (kind === 'villager') drawVillagers(context, cx, y, 14 * u, '#c9b88a', true);
      else { context.fillStyle = '#d8c594'; context.beginPath(); context.moveTo(cx - 5 * u, y); context.lineTo(cx, y - 5 * u); context.lineTo(cx + 5 * u, y); context.lineTo(cx + 4 * u, y + 5 * u); context.lineTo(cx - 4 * u, y + 5 * u); context.closePath(); context.fill(); }
      context.fillStyle = '#fff6d8'; context.fillText(texts[i], x + cell + 2 * u, y + .5 * u);
      x += widths[i] + gap;
    });
  }
}
