/* global window, document, performance, requestAnimationFrame */
import fs from "node:fs/promises";
import { summarize } from "./stutter_metrics.mjs";

// Serialized into the page by Playwright. The clock is the actual time at
// callback entry, independent of the timestamp Chromium passes to rAF.
export function capture({ name, seconds, pan, zoom }) {
  const canvas = document.querySelector("#scene");
  const api = window.aoeTelemetry;
  const mark = api.mark;
  const sync = performance.mark(`aoe-stutter-${name}`).startTime;
  const bySection = new Map([
    [1, [4, "socket_decode_apply"]],
    [2, [5, "render"]],
    [7, [14, "update"]],
    [8, [15, "interpolation"]],
    [9, [16, "culling"]],
    [10, [17, "terrain"]],
    [11, [18, "sprites_and_submission"]],
    [12, [19, "ui"]],
    [13, [20, "fog_and_packets"]],
    [30, [37, "terrain_samples"]],
    [31, [38, "terrain_foundations"]],
    [32, [39, "terrain_mesh"]],
    [33, [40, "terrain_decoration"]],
    [41, [48, "frame_body"]],
    [42, [49, "subscription_send"]],
    [43, [50, "chunk_requests"]],
    [44, [51, "chunk_decode"]],
    [45, [52, "chunk_install"]],
    [46, [53, "snapshot_apply"]],
    [47, [54, "network_decode"]],
    [55, [62, "native_streaming"]],
  ]);
  const starts = new Map();
  const frames = [];
  const restore = [];
  let spans = [],
    begin,
    painted = 0,
    waiting,
    first,
    done = false;
  function span(start, end, category) {
    if (begin !== undefined) spans.push([start - begin, end - begin, category]);
  }
  const gl = canvas.getContext("webgl2");
  for (const [category, names] of [
    [
      "buffer_allocation",
      ["createBuffer", "bufferData", "renderbufferStorage"],
    ],
    ["buffer_upload", ["bufferSubData"]],
    ["webgl_validation", ["getError", "clientWaitSync"]],
    [
      "texture_upload",
      [
        "texImage2D",
        "texImage3D",
        "texSubImage2D",
        "texSubImage3D",
        "texStorage3D",
        // This bridge calls finish only when transferring complete atlas pages.
        "finish",
      ],
    ],
    [
      "shader_compile",
      [
        "createShader",
        "createProgram",
        "compileShader",
        "linkProgram",
        "getShaderParameter",
        "getProgramParameter",
      ],
    ],
  ]) {
    for (const method of names) {
      const original = gl[method];
      gl[method] = function (...args) {
        const start = performance.now();
        try {
          return Reflect.apply(original, this, args);
        } finally {
          span(start, performance.now(), category);
        }
      };
      restore.push(() => {
        Reflect.deleteProperty(gl, method);
      });
    }
  }

  function move() {
    if (done) return;
    if (first !== undefined) {
      const start = performance.now();
      const phase = ((start - first) / 9000) % 2;
      const t = phase <= 1 ? phase : 2 - phase;
      const [from, to] = pan;
      window.__stutterClient.frame_bench_camera(
        from[0] + (to[0] - from[0]) * t,
        from[1] + (to[1] - from[1]) * t,
        zoom,
        false,
      );
      span(start, performance.now(), "pan_driver");
    }
    requestAnimationFrame(move);
  }
  if (pan) requestAnimationFrame(move);
  return new Promise((resolve) => {
    api.mark = function (section) {
      const now = performance.now();
      if (section === 0) {
        const current = Number(canvas.dataset.frame);
        const terrainBuilds = window.__stutterClient.frame_bench_take()[5];
        if (begin === undefined) first = now;
        else
          frames.push({
            at_ms: begin,
            ms: now - begin,
            spans,
            painted: current - painted,
            waiting,
            terrain_builds: Number(terrainBuilds),
            chunks_pending: Number(canvas.dataset.chunksPending),
          });
        if (now - first >= seconds * 1000) {
          done = true;
          api.mark = mark;
          for (const undo of restore) undo();
          resolve({ sync, frames });
        }
        begin = now;
        spans = [];
        painted = current;
        waiting = undefined;
      }
      if (section === 60) waiting = "gpu_queue";
      if (section === 61) waiting = "terrain";
      if (section === 63) waiting = "native_art";
      if (section === 22 && starts.has(4)) starts.get(4)[1] = "snapshot";
      const spec = bySection.get(section);
      if (spec) starts.set(spec[0], [now, spec[1]]);
      if (starts.has(section)) {
        const [start, category] = starts.get(section);
        span(start, now, category);
        starts.delete(section);
      }
      return mark.call(this, section);
    };
  });
}

export async function startTrace(page) {
  const cdp = await page.context().newCDPSession(page);
  await cdp.send("Tracing.start", {
    categories:
      "devtools.timeline,v8,blink.user_timing,toplevel,renderer.scheduler,cc,benchmark,gpu",
    transferMode: "ReturnAsStream",
  });
  return async (file) => {
    const complete = new Promise((resolve) =>
      cdp.once("Tracing.tracingComplete", resolve),
    );
    await cdp.send("Tracing.end");
    const { stream, dataLossOccurred } = await complete;
    if (dataLossOccurred) throw new Error("Chromium lost trace events");
    const chunks = [];
    try {
      for (;;) {
        const { data, eof } = await cdp.send("IO.read", { handle: stream });
        chunks.push(data);
        if (eof) break;
      }
    } finally {
      await cdp.send("IO.close", { handle: stream });
      await cdp.detach();
    }
    const trace = chunks.join("");
    await fs.writeFile(file, trace);
    return JSON.parse(trace).traceEvents;
  };
}

export function reportScene(name, { sync, frames }, trace) {
  const marker = trace.find(
    ({ name: label }) => label === `aoe-stutter-${name}`,
  );
  if (!marker) throw new Error(`no trace clock marker for ${name}`);
  const origin = marker.ts / 1000 - sync;
  const onMain = trace.filter(
    ({ pid, tid }) => pid === marker.pid && tid === marker.tid,
  );
  const addSpan = (start, end, category) => {
    for (const frame of frames) {
      if (frame.at_ms >= end) break;
      if (frame.ms > 50 && frame.at_ms + frame.ms > start) {
        frame.spans.push([start - frame.at_ms, end - frame.at_ms, category]);
      }
    }
  };
  for (const event of onMain) {
    if (event.ph !== "X" || !event.dur) continue;
    const category = traceCategory(event);
    if (!category) continue;
    const start = event.ts / 1000 - origin;
    addSpan(start, start + event.dur / 1000, category);
  }

  // rAF IDs are scoped to the document. The observed request-to-dispatch
  // interval gives a scheduler boundary. Any work inside takes precedence.
  const requests = new Map();
  const raf = onMain
    .filter(({ name }) => /^(Request|Cancel|Fire)AnimationFrame$/.test(name))
    .sort((a, b) => a.ts - b.ts);
  for (const { name: event, ts, args } of raf) {
    const { frame, id } = args?.data ?? {};
    if (frame === undefined || id === undefined) continue;
    const key = JSON.stringify([frame, id]);
    if (event === "RequestAnimationFrame") requests.set(key, ts);
    else {
      const requested = requests.get(key);
      requests.delete(key);
      if (event === "FireAnimationFrame" && requested !== undefined) {
        addSpan(
          requested / 1000 - origin,
          ts / 1000 - origin,
          "animation_dispatch_wait",
        );
      }
    }
  }
  const paints = frames.reduce((sum, row) => sum + row.painted, 0);
  if (!paints) throw new Error(`${name} did not render any frames`);
  return { paints, summary: summarize(frames), frames };
}

function traceCategory({ name, args }) {
  if (/^(?:MajorGC|MinorGC|V8\.GC)/.test(name)) return "gc";
  if (
    /^(?:RasterImplementation::ReadbackImagePixels|GLES2Implementation::ReadPixels)$/.test(
      name,
    )
  )
    return "gpu_readback";
  if (
    /^(?:ImplementationBase::WaitForCmd|CommandBufferProxyImpl::WaitForGetOffset)$/.test(
      name,
    )
  )
    return "gpu_command_wait";
  if (
    /^(?:Layout|UpdateLayoutTree|HitTest|Paint|PrePaint|Layerize|CompositeLayers)$/.test(
      name,
    )
  )
    return "browser_main";
  if (name === "FunctionCall" || name === "v8.callFunction") {
    const url = args?.data?.url;
    if (!url) return null;
    return url.endsWith("/pkg/aoe_client.js") ? "wasm_callback" : "js_callback";
  }
  if (
    /^(?:RunTask|ThreadControllerImpl::RunTask|ThreadPool_RunTask|EventDispatch|TimerFire|RunMicrotasks|FireAnimationFrame|BlinkScheduler_PerformMicrotaskCheckpoint)$/.test(
      name,
    )
  )
    return "browser_task";
  return null;
}
