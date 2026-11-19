import { actorColour } from '/minimap.js';
import { CELL, emptyGroup, tally } from '/navigation/landscape/groups.js';

export function drawMiniature(context, x, y, size, type, colour, art) {
  const sea = type & 4, economy = type & 2;
  const w = size * .5, h = size * (sea ? .4 : .44);
  const rim = new Path2D();
  if (sea) {
    rim.moveTo(x - w * .75, y - h);
    rim.lineTo(x + w * .75, y - h);
    rim.lineTo(x + w, y - h * .2);
    rim.lineTo(x + w * .67, y + h);
    rim.lineTo(x - w * .67, y + h);
    rim.lineTo(x - w, y - h * .2);
    rim.closePath();
  } else if (economy) {
    rim.arc(x, y, h, 0, Math.PI * 2);
  } else {
    rim.moveTo(x - w, y - h); rim.lineTo(x + w, y - h);
    rim.lineTo(x + w, y + h * .4);
    rim.quadraticCurveTo(x + w, y + h * .75, x, y + h * 1.2);
    rim.quadraticCurveTo(x - w, y + h * .75, x - w, y + h * .4);
    rim.closePath();
  }
  context.fillStyle = economy ? '#524126' : sea ? '#1c3140' : '#211b16';
  context.strokeStyle = '#17130f';
  context.lineWidth = size * .16;
  context.stroke(rim); context.fill(rim);

  if (art) {
    const draw = Math.min(w * (sea ? 1.82 : 1.72) / art.width, h * 1.8 / art.height);
    const wide = art.width * draw, high = art.height * draw;
    context.imageSmoothingEnabled = true;
    context.imageSmoothingQuality = 'high';
    context.drawImage(art, x - wide / 2, y - high / 2, wide, high);
  } else {
    // Readable category silhouettes while the pack loads or when art is absent.
    const s = size * .22;
    context.save();
    context.beginPath();
    if (sea) {
      context.moveTo(x - s, y + s * .35); context.lineTo(x + s, y + s * .35);
      context.lineTo(x + s * .65, y + s * .7); context.lineTo(x - s * .55, y + s * .7);
      context.closePath();
      context.moveTo(x - s * .2, y + s * .1); context.lineTo(x - s * .2, y - s);
      context.lineTo(x + s * .65, y + s * .1); context.closePath();
    } else if (economy) {
      context.arc(x, y - s * .47, s * .34, 0, Math.PI * 2);
      context.moveTo(x + s * .65, y + s * .7);
      context.arc(x, y + s * .7, s * .65, Math.PI * 2, Math.PI, true);
      context.closePath();
    } else {
      context.moveTo(x - s, y + s); context.lineTo(x + s, y - s);
      context.moveTo(x - s, y - s); context.lineTo(x + s, y + s);
      context.lineWidth = size * .09;
      context.lineCap = 'round';
      context.strokeStyle = '#ffedbf'; context.stroke();
    }
    context.fillStyle = '#ffedbf'; context.fill();
    context.restore();
  }
  // The owner colour is legible even on hulls with little player-colour art.
  context.lineWidth = Math.max(1, size * .07);
  context.strokeStyle = colour;
  if (economy) context.setLineDash([size * .18, size * .07]);
  context.stroke(rim);
  context.setLineDash([]);
}

function drawCount(context, x, y, text, size) {
  context.font = `bold ${Math.round(size * .4)}px Georgia,serif`;
  context.textAlign = 'center'; context.textBaseline = 'middle';
  context.lineWidth = Math.max(2, size * .14); context.lineJoin = 'round'; context.strokeStyle = '#140f08'; context.strokeText(text, x, y);
  context.fillStyle = '#fff6d8'; context.fillText(text, x, y);
}

const steps = [[0, 0]];
for (const ring of [1, 2]) for (const dy of [0, -1, 1, -2, 2]) for (const dx of [0, -1, 1, -2, 2]) {
  if (Math.max(Math.abs(dx), Math.abs(dy)) === ring) steps.push([dx * CELL, dy * CELL]);
}

export function drawUnits(context, groups, project, width, height, scale, space, place, artFor) {
  const picks = [];
  for (const group of groups) {
    const [x, y] = project(group.world);
    if (!(x >= 0 && y >= 0 && x < width && y < height)) continue;
    const px = x * scale, py = y * scale;
    const site = place(space.at(px, py));
    if (site?.owner === group.owner) {
      const combined = site.units.get(group.type) ?? emptyGroup();
      for (const [kind, count] of group.kinds) tally(combined, kind, count);
      site.units.set(group.type, combined);
      continue;
    }
    // Keep friendly and hostile tokens separate even when they share a cell.
    const cx = (Math.floor(px / CELL) + .5) * CELL;
    const cy = (Math.floor(py / CELL) + .5) * CELL;
    const sea = group.type & 4;
    const size = (group.friendly ? 35 : sea ? 32 : 29) / scale;
    const rect = ([dx, dy]) => [cx + dx - 19, cy + dy - 18, cx + dx + 19, cy + dy + 24];
    const step = steps.find(s => space.free(...rect(s))) ?? (group.friendly ? null : steps[0]);
    if (!step) continue;
    space.take(...rect(step), -2);
    const sx = (cx + step[0]) / scale, sy = (cy + step[1]) / scale;
    const art = artFor(group.kind);
    context.globalAlpha = group.friendly ? 1 : sea ? .94 : .85;
    drawMiniature(context, sx, sy, size, group.type, actorColour(group.colour), art);
    if (group.count > 1) drawCount(context, sx, sy + size * (sea ? .63 : .68), String(group.count), size);
    context.globalAlpha = 1;
    picks.push({ x: sx, y: sy, count: group.count, kind: group.kind, class: group.type, art: !!art, hostile: !group.friendly, world: group.world });
  }
  return picks;
}
