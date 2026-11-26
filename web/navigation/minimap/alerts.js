// Attacks that no screen shows: a star on the minimap for each battle, the place
// of the Space key and a few words for a screen reader. The client tells only
// new hits; those of a battle that goes on keep its star and raise no other.
import { initOverlay } from '/navigation/minimap/overlay.js';

const STAR_MS = 8000, BEAT_MS = 900, BEATS = 3;
const NEAR = 24;       // tiles from the first blow of a battle
const STARS = 4;       // at once: those of the newest battles
// Battles without a star are remembered too: otherwise five of them would take
// each other's place at every tick.
const BATTLES = 64;
const REACH_PX = 8;    // a click this near to a star is for it
const QUIET_MS = 10000, WORDS_MS = 5000;

const OUTLINE = `M${Array.from({ length: 16 }, (_, index) => {
  const angle = index * Math.PI / 8, radius = index % 2 ? 7 : 10;
  return [Math.sin(angle), -Math.cos(angle)].map(part => Math.round(radius * part * 100) / 100).join(' ');
}).join('L')}Z`;

// `region`: the live region; `latest` gets the place of each new battle.
export function initAlerts(layer, region, latest) {
  const overlay = initOverlay(layer);
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');
  let battles = [], timer, spoken = -Infinity, silence;

  function star() {
    const node = document.createElementNS(layer.namespaceURI, 'g');
    node.setAttribute('data-minimap-alert', '');
    const ring = reduced.matches ? ''
      : '<g class="alert-ring"><circle class="alert-ink" r="10"/><circle class="alert-core" r="10"/></g>';
    node.innerHTML = ring + ['alert-ink', 'alert-core', 'alert-line'].map(name =>
      `<path class="${name}" d="${OUTLINE}"/>`).join('');
    return node;
  }
  function beat(node) {
    node.querySelector('.alert-ring')?.animate(
      [{ transform: 'scale(1)', opacity: 1 }, { transform: 'scale(2.2)', opacity: 0 }],
      { duration: BEAT_MS, iterations: BEATS });
  }
  function show() {
    battles.forEach((battle, index) => {
      const newest = index >= battles.length - STARS;
      if (newest && !battle.star) {
        battle.star = star();
        overlay.add(battle.star, battle.place);
        beat(battle.star);
      } else if (!newest && battle.star) {
        overlay.remove(battle.star);
        battle.star = null;
      }
    });
  }
  // One timer for the first star to end. A blow only moves that moment.
  function wait() {
    clearTimeout(timer);
    if (!battles.length) return;
    const first = Math.min(...battles.map(battle => battle.until));
    timer = setTimeout(expire, Math.ceil(first - performance.now()));
  }
  function expire() {
    const now = performance.now();
    for (const { until, star: node } of battles) if (until <= now && node) overlay.remove(node);
    battles = battles.filter(battle => battle.until > now);
    show();
    wait();
  }
  // True when a battle is that near: all of those are fought on.
  function fought([x, y], now) {
    let known = false;
    for (const battle of battles) {
      if (Math.hypot(battle.place[0] - x, battle.place[1] - y) > NEAR) continue;
      battle.until = now + STAR_MS;
      known = true;
    }
    return known;
  }
  // The stalest one. At most STARS of them have a star, so some have none.
  function forget() {
    let stalest = null;
    for (const battle of battles) {
      if (!battle.star && (!stalest || battle.until < stalest.until)) stalest = battle;
    }
    battles.splice(battles.indexOf(stalest), 1);
  }
  function speak(now) {
    if (now - spoken < QUIET_MS) return;
    spoken = now;
    region.textContent = 'Under attack';
    clearTimeout(silence);
    silence = setTimeout(() => { region.textContent = ''; }, WORDS_MS);
  }
  return {
    // World x and y of each hit of one tick. The array belongs to the client.
    hits(places) {
      const now = performance.now(), fresh = [];
      // A throttled page runs its timers late.
      if (battles.some(battle => battle.until <= now)) expire();
      for (let at = 0; at + 1 < places.length; at += 2) {
        const place = [places[at], places[at + 1]];
        if (place.every(Number.isFinite) && !fought(place, now)) fresh.push(place);
      }
      // Only now: no battle struck in this tick is forgotten while a stale one is left.
      let raised = null;
      for (const place of fresh) {
        if (fought(place, now)) continue;
        if (battles.length >= BATTLES) forget();
        raised = { place, until: now + STAR_MS, star: null };
        battles.push(raised);
      }
      if (!raised) return;
      show();
      wait();
      latest(raised.place);
      speak(now);
    },
    at: (x, y) => overlay.nearest(x, y, REACH_PX),
    draw: overlay.draw,
    clear() {
      for (const battle of battles) if (battle.star) overlay.remove(battle.star);
      battles = [];
      wait();
    },
  };
}
