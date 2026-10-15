// A poster remains underneath every video until playback has actually begun.
// Ended clips retain their last frame through the crossfade; none loops.
export function createCinematic(root, finish) {
  const shots = Array.from(root.querySelectorAll('.intro-shot'));
  const stills = new URLSearchParams(location.search).get('introStills') === '1'
    || matchMedia('(prefers-reduced-motion: reduce)').matches;
  let timer;
  let active = -1;
  let running = false;
  const fallback = shot => {
    shot.querySelector('video').pause();
    shot.classList.remove('has-motion');
    shot.dataset.playback = 'fallback';
  };
  for (const shot of shots) {
    const video = shot.querySelector('video');
    video.muted = true;
    video.playbackRate = 1;
    video.addEventListener('error', () => fallback(shot));
    video.addEventListener('stalled', () => fallback(shot));
  }
  function activate(index) {
    if (active === index) return;
    if (active >= 0) shots[active].querySelector('video').pause();
    active = index;
    shots.forEach((shot, number) => shot.classList.toggle('active', number === index));
    const shot = shots[index];
    shot.classList.add('entered');
    if (stills) return;
    const video = shot.querySelector('video');
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
    shots.forEach(shot => shot.querySelector('video').pause());
  }
  return {
    stop,
    start() {
      running = true;
      if (!stills) shots.forEach(shot => {
        const video = shot.querySelector('video');
        video.src = video.dataset.src;
        video.load();
      });
      const started = performance.now();
      activate(0);
      timer = setInterval(() => {
        const elapsed = performance.now() - started;
        activate(Math.min(3, Math.floor(elapsed / 4000)));
        root.querySelector('#intro-line').textContent = elapsed < 6000
          ? 'Every story begins with a place.' : elapsed < 12000
            ? 'Across mountains, rivers and kingdoms.' : 'A world to discover. A history to shape.';
        if (elapsed >= 15500) root.querySelector('#entry-intro').classList.add('reveal');
        if (elapsed >= 18500) finish();
      }, 100);
    },
  };
}
