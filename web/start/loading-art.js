// Loading backdrop: one random series, its paintings one after another for
// SHOW_MS each, crossfading (web/start/README.md). Nothing is shown, and the
// screen keeps its bundled painting, when the local series art is absent.
import { pick, preload, seriesArt, shuffled } from './art.js';

export const SHOW_MS = 5000;

export function createLoadingArt(layer) {
  const slides = Array.from(layer.querySelectorAll('.loading-art-slide'));
  const caption = layer.querySelector('.loading-art-caption');
  let run = 0;
  let timer;
  let front = 0;

  function show(image, series, token) {
    if (token !== run) return;
    const next = slides[(front + 1) % slides.length];
    next.style.backgroundImage = `url('${image.url}')`;
    // Restart the slow drift on every painting.
    next.classList.remove('drifting');
    void next.offsetWidth;
    next.classList.add('drifting', 'visible');
    slides[front].classList.remove('visible');
    front = (front + 1) % slides.length;
    layer.dataset.image = image.url;
    layer.dataset.series = series.id;
    if (caption) caption.textContent = `${series.name}${image.culture ? ` · ${image.culture}` : ''}`;
  }

  async function start() {
    stop();
    const token = ++run;
    const all = await seriesArt();
    if (!all || token !== run) return;
    const series = pick(all);
    const queue = shuffled(series.images);
    layer.hidden = false;
    let at = 0;
    const advance = async () => {
      const image = queue[at++ % queue.length];
      try { await preload(image.url); } catch { timer = setTimeout(advance, 250); return; }
      if (token !== run) return;
      show(image, series, token);
      timer = setTimeout(advance, SHOW_MS);
      // Fetch the next painting while this one is on screen.
      preload(queue[at % queue.length].url).catch(() => {});
    };
    advance();
  }

  function stop() {
    run++;
    clearTimeout(timer);
  }
  return { start, stop };
}
