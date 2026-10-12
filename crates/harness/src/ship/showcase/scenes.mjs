// Rendered showcase scenes beyond cards, plain terminals and browsers
// (imported by record.mjs): a side-by-side terminal, `timing`, `api` and
// `agent-session`. Like cards they are drawn in-page from the plan's data, run
// nothing and load nothing. Pacing mirrors scenes.rs (HOLD_MS, TYPE_MS, ...):
// change both together. Each function returns when its animation is done; the
// recorder then holds the screen for the rest of the scene's planned length.

export const SCENE_STYLE = `
.panes { flex:1; display:flex; gap:20px; min-height:0; }
.pane { flex:1; display:flex; flex-direction:column; min-width:0; }
.pane .term { font-size:15px; padding:16px 18px; }
.pane .tag { align-self:flex-start; font-size:18px; padding:4px 14px; }
.race { margin:auto 0; font-family:"DejaVu Sans",sans-serif; }
.race h1 { font-size:38px; margin:0 0 10px; }
.race .command { font-family:"DejaVu Sans Mono",monospace; font-size:22px; color:var(--blue); background:var(--panel); border:1px solid #30363d; border-radius:10px; padding:12px 18px; margin-bottom:34px; white-space:pre-wrap; }
.lane { display:flex; align-items:center; margin:14px 0; }
.lane .name { width:130px; font-size:22px; font-weight:bold; } .lane .track { flex:1; height:46px; background:var(--panel); border-radius:8px; overflow:hidden; border:1px solid #30363d; }
.lane .fill { height:100%; width:0; } .lane .time { width:190px; text-align:right; font:bold 28px "DejaVu Sans Mono",monospace; }
.fill.before { background:#8e2a31; } .fill.after { background:#2ea043; }
.ratio { margin-top:26px; text-align:center; font-size:56px; font-weight:bold; color:var(--green); opacity:0; transition:opacity .4s; } .ratio.slower { color:var(--red); } .ratio.show { opacity:1; }
.extras { margin-top:22px; font-size:22px; } .extras div { margin:6px 0; opacity:0; transition:opacity .3s; } .extras div.show { opacity:1; }
.extras .lbl { display:inline-block; width:260px; color:var(--dim); } .extras .arrow { color:var(--dim); margin:0 10px; }
.req { font-size:22px; margin:0 0 14px; } .req .method { color:var(--blue); font-weight:bold; }
.reqbody { background:var(--panel); border:1px solid #30363d; border-radius:10px; padding:10px 16px; font-size:15px; white-space:pre-wrap; margin-bottom:12px; max-height:150px; overflow:hidden; }
.status { font:bold 20px "DejaVu Sans",sans-serif; margin-left:10px; } .status.changed { text-decoration:underline; }
.row { white-space:pre-wrap; min-height:1.55em; padding:0 6px; border-radius:4px; } .row.del { background:#3d1418; color:var(--red); } .row.add { background:#12301b; color:var(--green); }
.step { margin:0 0 10px; padding:6px 12px; border-left:4px solid #30363d; white-space:pre-wrap; word-break:break-word; }
.step .role { display:inline-block; min-width:74px; margin-right:12px; font:bold 15px "DejaVu Sans",sans-serif; letter-spacing:.06em; }
.step.user { border-color:var(--blue); } .step.user .role { color:var(--blue); }
.step.agent .role { color:var(--fg); } .step.agent { border-color:var(--fg); }
.step.tool { border-color:#d2a8ff; color:#d2a8ff; font-size:17px; } .step.tool .role { color:#d2a8ff; }
.step.hook { border-color:#e3b341; } .step.hook .role { color:#e3b341; }
.step.denied { border-color:var(--red); background:#2a1215; } .step.denied .role, .badge.denied { color:var(--red); }
.badge { margin-left:12px; font:bold 16px "DejaVu Sans",sans-serif; } .badge.allowed { color:var(--green); }
.term.session { font-size:18px; line-height:1.4; }`;

const TAGS = { before: "BEFORE", after: "AFTER" };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const TYPE_MS = 22;
const SESSION_TYPE_MS = 10;
const SESSION_PAUSE_MS = 400;
const RACE_MS = 2500;
const TIMING_INTRO_MS = 1000;
const EXTRA_MS = 400;
const REPLY_LINE_MS = 300;
const REQUEST_PAUSE_MS = 500;

// Type the lines into the element `id`, as the single-pane terminal does.
function play(page, id, lines) {
  return page.evaluate(async ({ id, lines }) => {
    const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
    const term = document.getElementById(id);
    for (const { style, text } of lines) {
      const row = document.createElement("div");
      row.className = style;
      term.appendChild(row);
      term.scrollTop = term.scrollHeight;
      if (style === "cmd") {
        for (const ch of text) { row.textContent += ch; term.scrollTop = term.scrollHeight; await sleep(22); }
        await sleep(500);
      } else {
        row.textContent = text;
        term.scrollTop = term.scrollHeight;
        await sleep(text ? 650 : 200);
      }
    }
  }, { id, lines });
}

// Two panes, before left and after right, typed at the same time.
export async function compare(page, scene, { stage, escape }) {
  const pane = (side) => `<div class="pane"><div><span class="tag ${side}">${TAGS[side]}</span></div><div class="term" id="${side}"></div></div>`;
  await page.setContent(stage(`<div class="stage"><div><span class="tag neutral">${escape(scene.tag)}</span></div><div class="caption">${escape(scene.caption)}</div><div class="panes">${pane("before")}${pane("after")}</div></div>`));
  await Promise.all([play(page, "before", scene.compare.before), play(page, "after", scene.compare.after)]);
}

export function ratioText(beforeMs, afterMs) {
  const faster = beforeMs / afterMs;
  if (faster >= 1.05) return { text: `${faster.toFixed(1)}× faster`, slower: false };
  if (faster <= 1 / 1.05) return { text: `${(1 / faster).toFixed(1)}× slower`, slower: true };
  return { text: "no change", slower: false };
}

// Two bars racing in scaled real time: the slower build takes RACE_MS, the
// faster one finishes proportionally earlier.
export async function timing(page, scene, { stage, escape }) {
  const unit = scene.unit ?? "ms";
  const ratio = ratioText(scene.before_ms, scene.after_ms);
  const lane = (side) => `<div class="lane"><div class="name ${side}">${TAGS[side]}</div><div class="track"><div class="fill ${side}" id="f-${side}"></div></div><div class="time" id="t-${side}">0 ${unit}</div></div>`;
  const extras = (scene.extra ?? []).map((e) => `<div><span class="lbl">${escape(e.label)}</span>${escape(e.before)}<span class="arrow">→</span>${escape(e.after)}</div>`).join("");
  await page.setContent(stage(`<div class="stage"><div class="race"><h1>${escape(scene.label)}</h1><div class="command">$ ${escape(scene.command)}</div>${lane("before")}${lane("after")}<div class="ratio${ratio.slower ? " slower" : ""}" id="ratio">${escape(ratio.text)}</div><div class="extras" id="extras">${extras}</div></div></div>`));
  await sleep(TIMING_INTRO_MS);
  await page.evaluate(async ({ before, after, unit, raceMs }) => {
    const slow = Math.max(before, after);
    const show = (ms) => (unit === "s" ? `${(ms / 1000).toFixed(2)} s` : `${Math.round(ms)} ms`);
    const start = performance.now();
    await new Promise((done) => {
      const frame = () => {
        const shown = Math.min(slow, ((performance.now() - start) / raceMs) * slow);
        for (const [side, real] of [["before", before], ["after", after]]) {
          const ms = Math.min(real, shown);
          document.getElementById(`f-${side}`).style.width = `${(ms / slow) * 100}%`;
          document.getElementById(`t-${side}`).textContent = show(ms);
        }
        if (shown < slow) requestAnimationFrame(frame); else done();
      };
      frame();
    });
    document.getElementById("ratio").classList.add("show");
  }, { before: scene.before_ms, after: scene.after_ms, unit, raceMs: RACE_MS });
  for (const row of await page.$$("#extras div")) {
    await row.evaluate((el) => el.classList.add("show"));
    await sleep(EXTRA_MS);
  }
}

// Lines of `mine` that `other` lacks (counting repeats): the changed ones.
export function changed(mine, other) {
  const left = new Map();
  for (const line of other) left.set(line, (left.get(line) ?? 0) + 1);
  return mine.map((line) => {
    const n = left.get(line) ?? 0;
    if (n > 0) left.set(line, n - 1);
    return n === 0;
  });
}

// The request typed out, then both responses revealed line by line with the
// lines that differ marked red (before) and green (after).
export async function api(page, scene, { stage, escape }) {
  const lines = { before: scene.before.body.split("\n"), after: scene.after.body.split("\n") };
  const marks = { before: changed(lines.before, lines.after), after: changed(lines.after, lines.before) };
  const differs = scene.before.status !== scene.after.status;
  const panes = ["before", "after"].map((side) => `<div class="pane"><div><span class="tag ${side}">${TAGS[side]}</span><span class="status${differs ? " changed" : ""}">${scene[side].status}</span></div><div class="term" id="${side}"></div></div>`).join("");
  const body = scene.request ? `<div class="reqbody">${escape(scene.request)}</div>` : "";
  await page.setContent(stage(`<div class="stage"><div class="caption">${escape(scene.caption)}</div><div class="req"><span class="method">${escape(scene.method)}</span> <span id="path"></span></div>${body}<div class="panes">${panes}</div></div>`));
  await page.evaluate(async ({ text, ms }) => {
    const el = document.getElementById("path");
    for (const ch of text) { el.textContent += ch; await new Promise((r) => setTimeout(r, ms)); }
  }, { text: scene.path, ms: TYPE_MS });
  await sleep(REQUEST_PAUSE_MS);
  const rows = Math.max(lines.before.length, lines.after.length);
  for (let i = 0; i < rows; i++) {
    await page.evaluate(({ i, lines, marks }) => {
      for (const side of ["before", "after"]) {
        if (i >= lines[side].length) continue;
        const row = document.createElement("div");
        row.className = `row${marks[side][i] ? (side === "before" ? " del" : " add") : ""}`;
        row.textContent = lines[side][i];
        document.getElementById(side).appendChild(row);
      }
    }, { i, lines, marks });
    await sleep(REPLY_LINE_MS);
  }
}

// A headless agent session replayed as a transcript: each step typed under its
// role label; hook and tool steps carry their verdict once typed.
export async function agentSession(page, scene, { stage, escape }) {
  await page.setContent(stage(`<div class="stage"><div><span class="tag neutral">${escape(scene.tag)}</span></div><div class="caption">${escape(scene.caption)}</div><div class="term session" id="t"></div></div>`));
  for (const step of scene.steps) {
    await page.evaluate(async ({ step, ms }) => {
      const sleep = (r) => new Promise((done) => setTimeout(done, r));
      const term = document.getElementById("t");
      const row = document.createElement("div");
      row.className = `step ${step.role}${step.verdict === "denied" ? " denied" : ""}`;
      const role = document.createElement("span");
      role.className = "role";
      role.textContent = step.role.toUpperCase();
      const text = document.createElement("span");
      row.append(role, text);
      term.appendChild(row);
      for (const ch of step.text) { text.textContent += ch; term.scrollTop = term.scrollHeight; await sleep(ms); }
      if (step.verdict) {
        const badge = document.createElement("span");
        badge.className = `badge ${step.verdict}`;
        badge.textContent = step.verdict === "denied" ? "✗ DENIED" : "✓ ALLOWED";
        row.appendChild(badge);
      }
      term.scrollTop = term.scrollHeight;
    }, { step, ms: SESSION_TYPE_MS });
    await sleep(SESSION_PAUSE_MS);
  }
}
