// A poster remains underneath every video until playback has actually begun.
// Shots follow the clips' own length: the next shot begins fading in while the
// previous clip plays its last FADE_MS, so no last frame is held. None loops.
const FADE_MS = 700; // Keep equal to the .intro-shot opacity transition.
const DEFAULT_CLIP_MS = 3000;
const REVEAL_AFTER_END_MS = 500;
const FINISH_AFTER_REVEAL_MS = 3000;
const lines = [
  'Every story begins with a place.',
  'Across mountains, rivers and kingdoms.',
  'A world to discover. A history to shape.',
];

// Pure schedule: when each shot starts, when the last clip ends, caption times.
export function shotSchedule(clipMs) {
  const starts = [];
  clipMs.forEach((_, index) => starts.push(index === 0 ? 0 : starts[index - 1] + clipMs[index - 1] - FADE_MS));
  const last = clipMs.length - 1;
  const end = starts[last] + clipMs[last];
  const reveal = end + REVEAL_AFTER_END_MS;
  return {
    starts, end, reveal, finish: reveal + FINISH_AFTER_REVEAL_MS,
    captions: [0, (starts[1] + starts[2]) / 2, starts[last]],
  };
}

export function createCinematic(root, finish) {
  const shots = Array.from(root.querySelectorAll('.intro-shot'));
  const stills = new URLSearchParams(location.search).get('introStills') === '1'
    || matchMedia('(prefers-reduced-motion: reduce)').matches;
  let timer;
  let active = -1;
  let running = false;
  let started = 0;
  const pauseTimers = [];
  const videoOf = shot => shot.querySelector('video');
  const fallback = shot => {
    videoOf(shot).pause();
    shot.classList.remove('has-motion');
    shot.dataset.playback = 'fallback';
  };
  const clipMs = shot => {
    const seconds = videoOf(shot).duration;
    return Number.isFinite(seconds) && seconds > 0 ? seconds * 1000 : DEFAULT_CLIP_MS;
  };
  const now = () => String(Math.round(performance.now() - started));
  for (const shot of shots) {
    const video = videoOf(shot);
    video.muted = true;
    video.playbackRate = 1;
    video.addEventListener('error', () => fallback(shot));
    video.addEventListener('stalled', () => fallback(shot));
    // Measurement hook: when the clip actually finished, relative to start().
    video.addEventListener('ended', () => { shot.dataset.endedAt = now(); });
  }
  function activate(index) {
    if (active === index) return;
    // The previous clip keeps playing through the crossfade, then stops.
    if (active >= 0) {
      const previous = videoOf(shots[active]);
      pauseTimers.push(setTimeout(() => previous.pause(), FADE_MS + 500));
    }
    active = index;
    shots.forEach((shot, number) => shot.classList.toggle('active', number === index));
    const shot = shots[index];
    shot.classList.add('entered');
    shot.dataset.activatedAt = now();
    if (stills) return;
    const video = videoOf(shot);
    try {
      video.currentTime = 0;
      video.play().then(() => {
        if (!running || active !== index) { video.pause(); return; }
        shot.classList.add('has-motion');
        shot.dataset.playback = 'video';
      }).catch(() => fallback(shot));
    } catch { fallback(shot); }
  }
  function stop() {
    running = false;
    clearInterval(timer);
    pauseTimers.splice(0).forEach(clearTimeout);
    shots.forEach(shot => videoOf(shot).pause());
  }
  return {
    stop,
    start() {
      running = true;
      if (!stills) shots.forEach(shot => {
        const video = videoOf(shot);
        video.src = video.dataset.src;
        video.load();
      });
      started = performance.now();
      activate(0);
      timer = setInterval(() => {
        const elapsed = performance.now() - started;
        const plan = shotSchedule(shots.map(clipMs));
        activate(plan.starts.filter(start => start <= elapsed).length - 1);
        root.querySelector('#intro-line').textContent =
          lines[plan.captions.filter(at => at <= elapsed).length - 1];
        if (elapsed >= plan.reveal) root.querySelector('#entry-intro').classList.add('reveal');
        if (elapsed >= plan.finish) finish();
      }, 50);
    },
  };
}
