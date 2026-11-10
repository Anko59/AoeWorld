// The first genuine zoom-out opens the land before the controls arrive. Match
// identity is a digest, never another stored copy of the private resume token.
export function initReveal() {
  const reducedMotion = matchMedia('(prefers-reduced-motion:reduce)');
  const panel = document.createElement('aside');
  panel.id = 'cockpit-reveal'; panel.hidden = true;
  panel.setAttribute('aria-label', 'Welcome to your kingdom');
  panel.innerHTML = '<div class="reveal-heading">Beyond your village</div><h2>A kingdom worth ruling</h2><p>The land is yours to discover.</p><button type="button" data-skip>Skip</button>';
  document.body.append(panel);
  const village = document.createElement('canvas');
  village.id = 'cockpit-village'; village.hidden = true;
  village.width = village.height = 1;
  document.getElementById('scene').after(village);
  let seen = false, checking = false, visible = false, generation = 0;
  let latest = null, near = null, motion = null, finish = null, requested = null, timers = [];
  function dismiss(complete = false, restore = false) {
    timers.forEach(clearTimeout); timers = [];
    if (requested) { generation++; checking = false; seen = true; requested = null; }
    finish = motion ? complete ? motion.to : restore ? motion.requested : null : null;
    motion = null; panel.hidden = true;
    village.hidden = true; village.width = village.height = 1;
    document.documentElement.style.removeProperty('--reveal-scale');
    document.documentElement.style.removeProperty('--reveal-opacity');
    delete document.documentElement.dataset.cockpitReveal;
  }
  function phase(value) {
    document.documentElement.dataset.cockpitReveal = value;
    if (value !== 'pullback') { village.hidden = true; village.width = village.height = 1; }
  }
  function begin() {
    panel.hidden = false;
    const reduced = reducedMotion.matches;
    if (near && latest && !reduced) {
      const scene = document.getElementById('scene');
      village.width = scene.width; village.height = scene.height;
      village.getContext('2d')?.drawImage(scene,0,0);
      village.hidden = false;
      const [ww,wh,,,,,w,h] = latest;
      const zoom = Math.max(1/4096, Math.min(latest[8], (w-40)/(64*(ww+wh)), (h-100)/(32*(ww+wh))));
      motion = { from: near, to: [ww/2,wh/2,zoom], requested:[latest[2],latest[3],latest[8]], started: performance.now() };
      phase('pullback');
      timers.push(setTimeout(() => phase('land'),3200), setTimeout(() => phase('cockpit'),4400), setTimeout(() => dismiss(false,true),5800));
    } else {
      // A direct far-view link and reduced motion preserve the requested camera.
      phase('cockpit'); timers.push(setTimeout(() => dismiss(),2400));
    }
  }
  panel.querySelector('[data-skip]').addEventListener('click', () => dismiss(true));
  document.addEventListener('keydown', event => {
    if (panel.hidden || ['Shift','Control','Alt','Meta'].includes(event.key)) return;
    dismiss(event.key === 'Escape', /^F[1-8]$/.test(event.key) && document.documentElement.dataset.cockpitReveal === 'pullback');
    if (event.key === 'Escape') event.preventDefault();
  });
  document.addEventListener('pointerdown', event => {
    if (!panel.hidden && !panel.contains(event.target)) {
      dismiss();
    }
  }, true);
  document.addEventListener('wheel', () => { if (!panel.hidden) dismiss(); }, { passive:true });
  window.addEventListener('blur', () => dismiss());
  function link() { dismiss(); near = null; }
  window.addEventListener('hashchange', link);
  async function start() {
    checking = true;
    const current = generation;
    let token = '';
    try { token = localStorage.getItem('aoeworld.resume-token') ?? ''; } catch { /* In-memory fallback. */ }
    let key = `local-${generation}`;
    if (token) {
      const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(token));
      key = Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2,'0')).join('');
    }
    if (current !== generation) return;
    checking = false; seen = true;
    try {
      if (sessionStorage.getItem('aoeworld.cockpit-reveal') === key) {
        if (requested) finish = [requested[2],requested[3],requested[8]];
        requested = null; return;
      }
      sessionStorage.setItem('aoeworld.cockpit-reveal',key);
    } catch { /* This instance still presents at most once. */ }
    const original = requested;
    if (requested) latest = requested;
    requested = null;
    if (visible) begin();
    if (original && !motion) finish = [original[2],original[3],original[8]];
  }
  return {
    navigation(state) {
      latest = Array.from(state);
      if (motion) {
        const elapsed = performance.now()-motion.started, returning = elapsed > 4400;
        const t = Math.min(1, Math.max(0,(elapsed-(returning ? 4400 : 0))/(returning ? 1400 : 3200)));
        const ease = t*t*(3-2*t);
        const from = returning ? motion.to : motion.from, to = returning ? motion.requested : motion.to;
        // Stay over the village until the detailed renderer has handed off.
        const pan = returning ? ease : Math.max(0,(ease-.45)/.55);
        const zoom = Math.exp(Math.log(from[2])+(Math.log(to[2])-Math.log(from[2]))*ease);
        const ratio = zoom/motion.from[2];
        document.documentElement.style.setProperty('--reveal-scale',String(ratio));
        document.documentElement.style.setProperty('--reveal-opacity',String(Math.max(0,(ratio-.12)/.88)));
        return [from[0]+(to[0]-from[0])*pan, from[1]+(to[1]-from[1])*pan, zoom];
      }
      if (finish) { const result = finish; finish = null; return result; }
      if (requested) return near;
      const cutoff = .1*Math.max(1,Math.sqrt(state[6]*state[7]/(1280*720)));
      if (state[8] < cutoff && near && !seen && !reducedMotion.matches) {
        // Hold the last detailed view while the identity check completes. No
        // far frame is published and then undone by the camera pull-back.
        requested = latest; visible = true;
        if (!checking) start().catch(() => { checking = false; seen = true; latest = requested ?? latest; requested = null; begin(); });
        return near;
      }
      if (state[8] >= cutoff) near = [state[2],state[3],state[8]];
      return null;
    },
    show(value, state) {
      if (requested) return;
      if (value && state && !motion) latest = [state[0],state[1],state[2],state[3],NaN,NaN,state[4],state[5],state[6],0,1];
      const changed = visible !== value;
      visible = value;
      if (!value) { if (changed && !motion) dismiss(); }
      else if (!seen && !checking) start().catch(() => { checking = false; seen = true; if (visible) begin(); });
    },
    active: () => !!(motion || finish),
    link,
    interrupt() {
      const target = motion?.requested ?? (requested ? [requested[2],requested[3],requested[8]] : null);
      dismiss();
      return target;
    },
    reset() { generation++; seen = false; checking = false; requested = motion = finish = near = latest = null; dismiss(); },
  };
}
