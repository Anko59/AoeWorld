import assert from "node:assert/strict";
import test from "node:test";
import { summarize, selectCrowd } from "./stutter_metrics.mjs";

test("reports every interval and uses strict stall thresholds and elapsed time", () => {
  const frames = [5, 50, 51, 100, 200].map((ms) => ({ ms, spans: [] }));
  const report = summarize(frames);
  assert.equal(report.p50_ms, 51);
  assert.equal(report.p99_ms, 200);
  assert.equal(report.max_ms, 200);
  assert.equal(report.over_50.count, 3);
  assert.equal(report.over_100.count, 1);
  assert.equal(report.over_50.per_minute, (3 * 60000) / 406);
});

test("excludes nested work from its parent and leaves residual time visible", () => {
  const { over_50 } = summarize([
    {
      ms: 90,
      spans: [
        [0, 50, "render"],
        [5, 30, "terrain"],
        [30, 45, "buffer_upload"],
        [8, 14, "gc"],
        [60, 65, "unmeasured_browser"],
      ],
    },
  ]);
  assert.deepEqual(over_50.causes, { unattributed: 1 });
  const measured = summarize([
    {
      ms: 61,
      spans: [
        [0, 56, "render"],
        [5, 51, "terrain"],
      ],
    },
  ]);
  assert.equal(measured.over_50.causes.terrain, 1);
});

test("attributes terrain preparation requested by the HUD to terrain", () => {
  const report = summarize([
    {
      ms: 110,
      spans: [
        [0, 100, "ui"],
        [5, 90, "terrain_mesh"],
        [15, 25, "gc"],
      ],
    },
  ]);
  assert.deepEqual(report.stalls[0].exclusive_ms, {
    ui: 15,
    terrain: 75,
    gc: 10,
    unattributed: 10,
  });
  assert.deepEqual(report.over_100.causes, { terrain: 1 });
});

test("chooses a populated town with active combat over an isolated army", () => {
  const entity = (x, y, kind, task, category = "unit") => ({
    position: { x, y },
    kind,
    task,
    category,
  });
  const army = Array.from({ length: 10 }, () =>
    entity(90, 90, "Knight", "Attacking"),
  );
  const town = [
    entity(10, 10, "TownCenter", "Complete", "building"),
    entity(11, 10, "Knight", "Attacking"),
    entity(10, 12, "Villager", "Gathering"),
  ];
  const view = selectCrowd([...army, ...town], [10, 10]);
  assert.deepEqual(view, {
    x: 10,
    y: 10,
    units: 2,
    attackers: 1,
    buildings: 1,
  });
  assert.throws(() => selectCrowd(army, [10, 10]), /town with combat/);
});

test("projects only page main-thread GC onto overlapping long frames", async () => {
  const { reportScene } = await import("./stutter_capture.mjs");
  const frames = [
    { at_ms: 100, ms: 20, spans: [], painted: 1 },
    { at_ms: 120, ms: 110, spans: [], painted: 0 },
    { at_ms: 230, ms: 55, spans: [], painted: 1 },
  ];
  const marker = { name: "aoe-stutter-test", ts: 500000, pid: 4, tid: 2 };
  const gc = { ...marker, name: "MinorGC", ph: "X", ts: 520000, dur: 90000 };
  const scene = reportScene("test", { sync: 100, frames }, [
    marker,
    gc,
    { ...gc, tid: 3, ts: 600000, dur: 85000 },
  ]);
  assert.equal(scene.paints, 2);
  assert.deepEqual(scene.summary.over_50.causes, { gc: 1, unattributed: 1 });
  assert.deepEqual(scene.summary.over_100.causes, { gc: 1 });
});

test("attributes clock-aligned callbacks, task work and their nested causes without filling gaps", async () => {
  const { reportScene } = await import("./stutter_capture.mjs");
  const frames = [
    {
      at_ms: 100,
      ms: 100,
      painted: 1,
      spans: [[7, 40, "chunk_install"]],
    },
  ];
  const marker = { name: "aoe-stutter-test", ts: 500000, pid: 4, tid: 2 };
  const event = (name, start, end, extra = {}) => ({
    ...marker,
    name,
    ph: "X",
    ts: marker.ts + start * 1000,
    dur: (end - start) * 1000,
    ...extra,
  });
  const report = reportScene("test", { sync: 100, frames }, [
    marker,
    event("ThreadControllerImpl::RunTask", -5, 60),
    event("FunctionCall", 3, 45, {
      args: { data: { url: "http://localhost/pkg/aoe_client.js" } },
    }),
    event("MinorGC", 20, 30),
    event("FunctionCall", 65, 70, {
      args: { data: { url: "http://localhost/navigation/strategic.js" } },
    }),
    event("ThreadControllerImpl::RunTask", 70, 100, { tid: 3 }),
  ]);
  assert.deepEqual(report.summary.stalls[0].exclusive_ms, {
    browser_task: 18,
    wasm_callback: 9,
    chunk_install: 23,
    gc: 10,
    js_callback: 5,
    unattributed: 35,
  });
  assert.equal(report.summary.stalls[0].attributed_fraction, 0.65);
});

test("accounts for time awaiting a matching rAF dispatch, including sorting and cancellation", async () => {
  const { reportScene } = await import("./stutter_capture.mjs");
  const frames = [{ at_ms: 10, ms: 100, spans: [], painted: 1 }];
  const marker = { name: "aoe-stutter-test", ts: 99000, pid: 4, tid: 2 };
  const event = (name, at, id, frame = "page") => ({
    ...marker,
    name,
    ts: 100000 + at * 1000,
    args: { data: { id, frame } },
  });
  const report = reportScene("test", { sync: 9, frames }, [
    marker,
    event("FireAnimationFrame", 90, 1),
    event("RequestAnimationFrame", 15, 1),
    event("FireAnimationFrame", 95, 2, "other-page"),
    event("RequestAnimationFrame", 1, 2),
    event("RequestAnimationFrame", 0, 3),
    event("CancelAnimationFrame", 5, 3),
    event("FireAnimationFrame", 98, 3),
  ]);
  assert.deepEqual(report.summary.stalls[0].exclusive_ms, {
    unattributed: 25,
    animation_dispatch_wait: 75,
  });
});

test("assigns blocking pixel readback ahead of the callback and scheduling envelopes", async () => {
  const { reportScene } = await import("./stutter_capture.mjs");
  const marker = { name: "aoe-stutter-test", ts: 500000, pid: 4, tid: 2 };
  const span = (name, ts, dur) => ({ ...marker, name, ph: "X", ts, dur });
  const report = reportScene(
    "test",
    { sync: 100, frames: [{ at_ms: 100, ms: 200, painted: 1, spans: [] }] },
    [
      marker,
      span("BlinkScheduler_PerformMicrotaskCheckpoint", 500000, 190000),
      span("RasterImplementation::ReadbackImagePixels", 505000, 180000),
      span("ImplementationBase::WaitForCmd", 510000, 170000),
    ],
  );
  assert.deepEqual(report.summary.stalls[0].exclusive_ms, {
    browser_task: 10,
    gpu_readback: 180,
    unattributed: 10,
  });
});
