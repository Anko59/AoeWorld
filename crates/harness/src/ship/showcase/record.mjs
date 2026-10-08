// Records a showcase plan (plan.json, written by `aoe-harness showcase`) in one
// Playwright take: title cards and scripted terminals are drawn in-page;
// browser scenes film the app under development at plan.app, the one origin
// allowed: every other request and WebSocket is aborted (AppOrigin::allows).
// Each scene holds the screen for exactly its planned duration, counted from
// after its page has loaded (longer only if a step is slow), so steps are
// counted once. Writes silent.webm and timings.json: the blank lead-in and each
// scene's actual on-screen length, page loading included, in milliseconds,
// which the audio track is padded to.
import { chromium } from "playwright";
import { constants, copyFileSync, readFileSync, unlinkSync, writeFileSync } from "node:fs";

const plan = JSON.parse(readFileSync("plan.json", "utf8"));
const app = plan.app ?? null;
const dir = process.cwd();
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const escape = (s) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

const STYLE = `
:root { --bg:#0d1117; --panel:#161b22; --fg:#e6edf3; --dim:#8b949e; --red:#ff7b72; --green:#7ee787; --blue:#79c0ff; }
html,body { margin:0; height:100%; background:var(--bg); color:var(--fg); font-family:"DejaVu Sans Mono",monospace; }
.stage { position:absolute; inset:0; display:flex; flex-direction:column; padding:36px 56px; box-sizing:border-box; }
.card { margin:auto; text-align:center; font-family:"DejaVu Sans",sans-serif; }
.card h1 { font-size:46px; margin:0 0 18px; } .card p { font-size:24px; color:var(--dim); margin:8px 0; }
.tag { display:inline-block; padding:6px 16px; border-radius:20px; font-family:"DejaVu Sans",sans-serif; font-size:22px; font-weight:bold; margin-bottom:14px; }
.before { background:#3d1418; color:var(--red); } .after { background:#12301b; color:var(--green); } .neutral { background:#1f2a3a; color:var(--blue); }
.caption { font-family:"DejaVu Sans",sans-serif; font-size:26px; margin:0 0 14px; }
.term { flex:1; background:var(--panel); border-radius:12px; padding:22px 26px; font-size:19px; line-height:1.55; white-space:pre-wrap; overflow:hidden; border:1px solid #30363d; }
.cmd { color:var(--blue); } .dim { color:var(--dim); } .bad { color:var(--red); } .good { color:var(--green); }`;

const TAGS = { before: "BEFORE", after: "AFTER", neutral: "" };

function stage(body) {
  return `<!doctype html><html><head><meta charset="utf-8"><title>${escape(plan.title)}</title><style>${STYLE}</style></head><body>${body}</body></html>`;
}

async function card(page, scene) {
  const lines = (scene.lines ?? []).map((l) => `<p>${escape(l)}</p>`).join("");
  await page.setContent(stage(`<div class="stage"><div class="card"><h1>${escape(scene.heading)}</h1>${lines}</div></div>`));
}

async function terminal(page, scene) {
  const tone = scene.tone ?? "neutral";
  const tag = `${TAGS[tone] ? `${TAGS[tone]} · ` : ""}${escape(scene.tag)}`;
  await page.setContent(stage(`<div class="stage"><div><span class="tag ${tone}">${tag}</span></div><div class="caption">${escape(scene.caption)}</div><div class="term" id="t"></div></div>`));
  for (const line of scene.lines) {
    await page.evaluate(async ({ style, text }) => {
      const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
      const row = document.createElement("div");
      row.className = style;
      const term = document.getElementById("t");
      term.appendChild(row);
      const follow = () => { term.scrollTop = term.scrollHeight; };
      follow();
      if (style === "cmd") {
        for (const ch of text) { row.textContent += ch; follow(); await sleep(22); }
        await sleep(500);
      } else {
        row.textContent = text;
        follow();
        await sleep(text ? 650 : 200);
      }
    }, line);
  }
}

function within(value, prefix) {
  try {
    return new URL(value).href.startsWith(prefix);
  } catch {
    return false;
  }
}

// Load the app, then caption it; loading is measured, not planned.
async function open(page, scene) {
  await page.goto(scene.url, { waitUntil: "load", timeout: 30_000 });
  await page.evaluate((text) => {
    const el = document.createElement("div");
    el.textContent = text;
    Object.assign(el.style, { position: "fixed", left: 0, right: 0, bottom: 0, padding: "12px 24px", background: "rgba(13,17,23,.85)", color: "#e6edf3", font: '24px "DejaVu Sans",sans-serif', zIndex: 2147483647, pointerEvents: "none" });
    document.body.appendChild(el);
  }, scene.caption);
}

async function steps(page, scene) {
  for (const step of scene.steps ?? []) {
    if ("click" in step) await page.click(step.click, { timeout: 10_000 });
    else if ("key" in step) await page.keyboard.press(step.key);
    else if ("text" in step) await page.keyboard.type(step.text, { delay: 22 });
    else if ("wait_ms" in step) await sleep(step.wait_ms);
  }
}

const browser = await chromium.launch();
const context = await browser.newContext({
  viewport: { width: 1280, height: 720 },
  recordVideo: { dir, size: { width: 1280, height: 720 } },
  serviceWorkers: "block",
});
await context.route("**/*", async (route) => {
  const url = route.request().url();
  if (url === "about:blank" || url.startsWith("data:")) await route.continue();
  else if (app !== null && within(url, app.http)) await route.continue();
  else await route.abort("blockedbyclient");
});
await context.routeWebSocket(/.*/, async (ws) => {
  if (app !== null && within(ws.url(), app.ws)) ws.connectToServer();
  else await ws.close();
});
const started = Date.now();
const page = await context.newPage();
const timings = { lead_in_ms: null, scenes_ms: [] };
for (const scene of plan.scenes) {
  const sceneStart = Date.now();
  if (timings.lead_in_ms === null) timings.lead_in_ms = sceneStart - started;
  if (scene.kind === "browser") await open(page, scene);
  else if (page.url() !== "about:blank") await page.goto("about:blank");
  const begin = Date.now();
  if (scene.kind === "card") await card(page, scene);
  else if (scene.kind === "terminal") await terminal(page, scene);
  else await steps(page, scene);
  const left = scene.duration_ms - (Date.now() - begin);
  if (left > 0) await sleep(left);
  timings.scenes_ms.push(Date.now() - sceneStart);
}
const video = page.video();
await context.close();
await browser.close();
const videoPath = await video.path();
copyFileSync(videoPath, `${dir}/silent.webm`, constants.COPYFILE_EXCL);
unlinkSync(videoPath);
writeFileSync(`${dir}/timings.json`, JSON.stringify(timings), { flag: "wx" });
