import { entityIcon, panelArt } from '/interface/ui-art.js';

// A small, dismissible occasion. Match identity is a digest, never another copy
// of the private resume token. Session storage keeps reloads from replaying it.
export function initReveal() {
  const panel = document.createElement('aside');
  panel.id = 'cockpit-reveal'; panel.hidden = true;
  panel.setAttribute('aria-label', 'Welcome to your kingdom');
  panel.innerHTML = `<div class="reveal-heading">A wider world</div><div class="reveal-art"></div>
    <h2></h2><p></p><footer><span></span><button type="button" data-next>Next</button><button type="button" data-skip>Skip tour</button></footer>`;
  document.body.append(panel);
  const steps = [
    ['kingdom', 'Armies & kingdoms', 'Your village belongs to a wider world. Read your kingdom’s real resources and armies at a glance. Select a General to command his army; the layers reveal what you know.', 'Castle', 'General'],
    ['commands', 'Units here & building', 'Choose Units here to raise troops at a destination. Choose a building and a site to send nearby villagers. Your kingdom can grow from this view.', 'Barracks', 'Villager'],
    ['jumps', 'Jump keys', 'H returns to your main base. End finds your largest reported army; Space finds the latest alert. Ctrl+F1–F4 saves a view; Shift+F1–F4 returns to it.', 'TownCenter', 'Scout'],
  ];
  let seen = false, checking = false, visible = false, generation = 0, step = 0, timer;
  function dismiss() {
    clearTimeout(timer); panel.hidden = true;
    delete document.documentElement.dataset.cockpitReveal;
  }
  function render() {
    clearTimeout(timer);
    const [target, title, text, ...art] = steps[step];
    panel.querySelector('h2').textContent = title;
    panel.querySelector('p').textContent = text;
    panel.querySelector('.reveal-art').innerHTML = art.map(entityIcon).join('');
    panel.querySelector('footer span').textContent = `${step + 1} / ${steps.length}`;
    panel.querySelector('[data-next]').textContent = step === steps.length - 1 ? 'Command your kingdom' : 'Next';
    panelArt(panel, 'selection');
    document.documentElement.dataset.cockpitReveal = target;
    panel.hidden = false;
    if (step < steps.length - 1) timer = setTimeout(next, 4200);
  }
  function next() { if (++step < steps.length) render(); else dismiss(); }
  panel.querySelector('[data-next]').addEventListener('click', next);
  panel.querySelector('[data-skip]').addEventListener('click', dismiss);
  document.addEventListener('keydown', event => {
    if (!panel.hidden && event.key === 'Escape') { dismiss(); event.preventDefault(); }
  });
  // Interaction immediately ends the tour; the game never waits for it.
  document.addEventListener('pointerdown', event => {
    if (!panel.hidden && !panel.contains(event.target)) dismiss();
  });
  panel.addEventListener('pointerenter', () => clearTimeout(timer));
  async function start() {
    checking = true;
    const current = generation;
    let token = '';
    try { token = localStorage.getItem('aoeworld.resume-token') ?? ''; } catch { /* In-memory fallback. */ }
    let key = `local-${generation}`;
    if (token) {
      const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(token));
      key = Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, '0')).join('');
    }
    if (current !== generation) return;
    checking = false; seen = true;
    try {
      if (sessionStorage.getItem('aoeworld.cockpit-reveal') === key) return;
      sessionStorage.setItem('aoeworld.cockpit-reveal', key);
    } catch { /* This instance still presents at most once. */ }
    if (visible) { step = 0; render(); }
  }
  return {
    show(value) {
      visible = value;
      if (!value) dismiss();
      else if (!seen && !checking) start().catch(() => { checking = false; seen = true; if (visible) { step = 0; render(); } });
    },
    reset() { generation++; seen = false; checking = false; dismiss(); },
  };
}
