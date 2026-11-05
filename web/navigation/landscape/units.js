import { kindSprite } from '/interface/ui-art.js';
import { actorColour } from '/minimap.js';

// Map pieces for moving actors: soldiers are heraldic shields with crossed
// swords, villagers are round portraits with the real Villager icon. The two
// shapes differ in outline, interior and colour weight so they read at a glance.
let portrait = null;
function villagerPortrait() {
  if (portrait) return portrait;
  const art = kindSprite('Villager', 64);
  if (!art) return null;
  portrait = document.createElement('canvas'); portrait.width = portrait.height = 64;
  const context = portrait.getContext('2d');
  context.beginPath(); context.arc(32, 32, 30, 0, Math.PI * 2); context.clip();
  context.drawImage(art, 0, 0, 64, 64);
  return portrait;
}
let helm = null;
const soldierPortrait = () => helm ??= kindSprite('Militia', 64);
export function drawShield(context, x, y, size, colour, quiet = false) {
  const w = size * .5, top = y - size * .5, bottom = y + size * .55;
  context.beginPath();
  context.moveTo(x - w, top); context.lineTo(x + w, top); context.lineTo(x + w, y + size * .1);
  context.quadraticCurveTo(x + w, y + size * .38, x, bottom); context.quadraticCurveTo(x - w, y + size * .38, x - w, y + size * .1);
  context.closePath();
  context.fillStyle = colour; context.fill();
  const art = soldierPortrait();
  if (art) {
    // The real Militia portrait inside the owner-coloured heraldic shield.
    context.save(); context.clip();
    context.drawImage(art, x - w * .86, top + size * .04, w * 1.72, w * 1.72);
    context.restore();
  } else {
    context.strokeStyle = '#fff6d8'; context.lineWidth = Math.max(1.5, size * .09); context.lineCap = 'round';
    const s = size * .22;
    context.beginPath(); context.moveTo(x - s, y - s * 1.1); context.lineTo(x + s, y + s * 1.1);
    context.moveTo(x + s, y - s * 1.1); context.lineTo(x - s, y + s * 1.1); context.stroke();
    context.lineCap = 'butt';
  }
  context.lineWidth = Math.max(quiet ? 1.5 : 2.5, size * .12); context.strokeStyle = colour; context.beginPath();
  context.moveTo(x - w, top); context.lineTo(x + w, top); context.lineTo(x + w, y + size * .1);
  context.quadraticCurveTo(x + w, y + size * .38, x, bottom); context.quadraticCurveTo(x - w, y + size * .38, x - w, y + size * .1);
  context.closePath(); context.stroke();
  context.lineWidth = Math.max(1, size * .05); context.strokeStyle = '#1b140c'; context.stroke();
}
export function drawVillagers(context, x, y, size, colour, quiet = false) {
  const r = size * .42, art = villagerPortrait();
  context.beginPath(); context.arc(x, y, r + 1.5, 0, Math.PI * 2);
  context.fillStyle = '#1b140c'; context.fill();
  if (art) context.drawImage(art, x - r, y - r, r * 2, r * 2);
  else { context.beginPath(); context.arc(x, y, r, 0, Math.PI * 2); context.fillStyle = '#9d8a57'; context.fill(); }
  context.beginPath(); context.arc(x, y, r + .5, 0, Math.PI * 2);
  context.lineWidth = Math.max(quiet ? 1.5 : 2, size * .1); context.strokeStyle = colour; context.stroke();
}
export function drawCount(context, x, y, text, size) {
  context.font = `bold ${Math.round(size * .5)}px Georgia,serif`;
  context.textAlign = 'center'; context.textBaseline = 'middle';
  context.lineWidth = Math.max(2, size * .22); context.lineJoin = 'round'; context.strokeStyle = '#140f08'; context.strokeText(text, x, y);
  context.fillStyle = '#fff6d8'; context.fillText(text, x, y);
}

// Soldiers and villagers away from any place fold into one grid cell each, so
// a symbol is one army or one work party, never a pile. Units standing at a
// place count toward its plaque instead. Draw cost follows the number of cells
// on screen, not the number of units.
const CELL = 44;
export function drawUnits(context, records, project, width, height, scale, space, place, mine) {
  const bins = new Map();
  for (let at = 0; at + 4 < records.length; at += 5) {
    const kind = records[at];
    if (kind === 1) continue;
    const [sx, sy] = project([records[at + 1], records[at + 2]]);
    if (!(sx >= 0 && sy >= 0 && sx < width && sy < height)) continue;
    const px = sx * scale, py = sy * scale, owner = space.at(px, py);
    if (owner !== undefined) {
      const home = owner >= 0 ? place(owner) : null;
      if (home) home[kind === 2 ? 'workers' : 'soldiers']++;
      continue;
    }
    const cell = (Math.floor(px / CELL) + 64) * 4096 + Math.floor(py / CELL) + 64, key = cell * 4096 + (records[at + 3] & 4095);
    let bin = bins.get(key);
    if (!bin) { bin = { cx: (Math.floor(px / CELL) + .5) * CELL, cy: (Math.floor(py / CELL) + .5) * CELL, soldiers: 0, workers: 0, colour: records[at + 4], world: [0, 0], n: 0 }; bins.set(key, bin); }
    bin[kind === 2 ? 'workers' : 'soldiers']++; bin.n++;
    bin.world[0] += records[at + 1]; bin.world[1] += records[at + 2];
  }
  const picks = [], u = 1 / scale;
  for (const bin of [...bins.values()].sort((a, b) => b.soldiers - a.soldiers || a.cx - b.cx || a.cy - b.cy)) {
    const friendly = bin.colour === mine, S = (friendly ? 22 : 17) * u, V = (friendly ? 18 : 14) * u;
    const both = bin.soldiers > 0 && bin.workers > 0, half = (both ? 22 : 12) ;
    if (!space.free(bin.cx * scale - half, bin.cy * scale - 12, bin.cx * scale + half, bin.cy * scale + 16)) continue;
    space.take(bin.cx * scale - half, bin.cy * scale - 12, bin.cx * scale + half, bin.cy * scale + 16, -2);
    const colour = actorColour(bin.colour);
    context.globalAlpha = friendly ? 1 : .82;
    const X = bin.cx * u, Y = bin.cy * u, sx = X - (both ? 10 * u : 0), vx = X + (both ? 11 * u : 0);
    if (bin.soldiers) { drawShield(context, sx, Y, S, colour, !friendly); if (bin.soldiers > 1) drawCount(context, sx, Y + S * .78, String(bin.soldiers), S); }
    if (bin.workers) { drawVillagers(context, vx, Y + (both ? 2 * u : 0), V, colour, !friendly); if (bin.workers > 1) drawCount(context, vx, Y + V * .95, String(bin.workers), V * 1.1); }
    context.globalAlpha = 1;
    picks.push({ x: X, y: Y, count: bin.n, units: bin.n, workers: bin.workers, world: [bin.world[0] / bin.n, bin.world[1] / bin.n] });
  }
  return picks;
}
