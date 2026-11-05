import { kindSprite } from '/interface/ui-art.js';

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
export function drawShield(context, x, y, size, colour) {
  const w = size * .5, top = y - size * .5, bottom = y + size * .55;
  context.beginPath();
  context.moveTo(x - w, top); context.lineTo(x + w, top); context.lineTo(x + w, y + size * .1);
  context.quadraticCurveTo(x + w, y + size * .38, x, bottom); context.quadraticCurveTo(x - w, y + size * .38, x - w, y + size * .1);
  context.closePath();
  context.fillStyle = colour; context.fill();
  context.lineWidth = Math.max(2, size * .09); context.strokeStyle = '#1b140c'; context.stroke();
  context.lineWidth = Math.max(1, size * .04); context.strokeStyle = '#f6e7b4'; context.stroke();
  // Crossed swords.
  context.strokeStyle = '#fff6d8'; context.lineWidth = Math.max(1.5, size * .09); context.lineCap = 'round';
  const s = size * .22;
  context.beginPath(); context.moveTo(x - s, y - s * 1.1); context.lineTo(x + s, y + s * 1.1);
  context.moveTo(x + s, y - s * 1.1); context.lineTo(x - s, y + s * 1.1); context.stroke();
  context.lineCap = 'butt';
}
export function drawVillagers(context, x, y, size, colour) {
  const r = size * .42, art = villagerPortrait();
  context.beginPath(); context.arc(x, y, r + 1.5, 0, Math.PI * 2);
  context.fillStyle = '#1b140c'; context.fill();
  if (art) context.drawImage(art, x - r, y - r, r * 2, r * 2);
  else { context.beginPath(); context.arc(x, y, r, 0, Math.PI * 2); context.fillStyle = '#9d8a57'; context.fill(); }
  context.beginPath(); context.arc(x, y, r + .5, 0, Math.PI * 2);
  context.lineWidth = Math.max(2, size * .1); context.strokeStyle = colour; context.stroke();
}
export function drawCount(context, x, y, text, size) {
  context.font = `bold ${Math.round(size * .5)}px Georgia,serif`;
  context.textAlign = 'center'; context.textBaseline = 'middle';
  context.lineWidth = 3; context.strokeStyle = '#140f08'; context.strokeText(text, x, y);
  context.fillStyle = '#fff6d8'; context.fillText(text, x, y);
}
