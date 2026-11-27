import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import {
  apart,
  chart,
  contrast,
  landPage,
  luminance,
  type Land,
} from "./land-page.js";

test("explored cells keep a flat tone until their own map's land arrives", async ({
  page,
}) => {
  await landPage(page);
  const seen = await page.evaluate(async () => {
    const land = Reflect.get(window, "land") as Land;
    const { fog } = land.navigation;
    const panel = document.getElementById("minimap");
    const image = document.getElementById("minimap-terrain");
    if (!panel || !image) throw new Error("The minimap is missing");
    const { decodeMinimap } = await import("/minimap.js" as string);
    // The retry, five seconds after a failure, waits for this test instead.
    let retry: () => void = () => undefined;
    const later = window.setTimeout.bind(window);
    Reflect.set(window, "setTimeout", (run: () => void, delay?: number) => {
      if (delay !== 5000) return later(run, delay);
      retry = run;
      return 0;
    });
    const flat = (kind: number, samples: number) =>
      land.raster(samples, () => kind | (8 << 4));
    const read = () => {
      const { data, width } = land.composed();
      let drawn = 0;
      for (let at = 3; at < data.length; at += 4) if (data[at]) drawn++;
      const pixel = (x: number) => {
        const from = (256 * width + x) * 4;
        return Array.from(data.slice(from, from + 4));
      };
      return { drawn, left: pixel(100), middle: pixel(256), right: pixel(420) };
    };
    // Everything is in sight unless the test says otherwise.
    const look = async (state: (x: number, y: number) => number = () => 2) => {
      land.fog(state);
      await land.frames(3);
      return {
        ...read(),
        raster: panel.dataset["raster"],
        samples: panel.dataset["rasterSamples"],
        href: image.hasAttribute("href"),
      };
    };
    const drawn = async (frames: number) => {
      await land.frames(frames);
      return read().drawn;
    };

    // Land that is there before any fog call shows nothing.
    fog.land(decodeMinimap(flat(0, 8).buffer));
    const fresh = await drawn(3);
    fog.land(null);
    await land.open({ fine: flat(0, 8) });
    const opened = await drawn(3);

    const early = land.hold();
    const late = land.hold();
    const one = land.open({ coarse: early.reply, fine: late.reply });
    const waiting = await look();
    const before = land.paints();
    early.release(flat(11, 8));
    await land.until("coarse");
    await land.frames(4);
    const replaced = land.paints() - before;
    const coarse = await look();
    late.release(flat(0, 16));
    await one;
    const fine = await look();

    // A change of map empties the canvas within the frame that asks for it,
    // whatever was still to be composed for the last map.
    land.fog((x) => (x < 90 ? 2 : 1));
    const first = land.hold();
    const second = land.hold();
    const stages: Parameters<Land["open"]>[0] = {
      coarse: first.reply,
      fine: second.reply,
    };
    const two = land.open(stages);
    const emptied = [read().drawn, await drawn(2)];
    const pending = await look();
    first.release(503);
    second.release(503);
    await two;
    const again = land.hold();
    stages.coarse = stages.fine = again.reply;
    const failed = await look();
    const partial = await look((x) => (x < 90 ? 0 : x < 180 ? 1 : 2));
    retry();
    const retrying = await look();
    again.release(flat(2, 8));
    await land.until("ready");
    const retried = await look();

    // A refused refinement keeps the coarse land of its own map.
    await land.open({ coarse: flat(2, 8), fine: 503 });
    const refused = await look();

    // The land of a map that is already gone arrives last and is not drawn.
    const gone = land.hold();
    void land.open({ fine: gone.reply });
    await land.frames(2);
    const next = land.hold();
    const three = land.open({ fine: next.reply });
    gone.release(flat(0, 32));
    await land.frames(4);
    const stale = await look();
    next.release(flat(10, 32));
    await three;
    const own = await look();

    // Fog off on one map, then another: nothing of the first is kept, and land
    // that comes while the fog is off is there when the fog returns.
    fog(true, []);
    await land.frames(2);
    const unseen = land.hold();
    const four = land.open({ fine: unseen.reply });
    const cleared = [read().drawn, await drawn(2)];
    fog(true, []);
    unseen.release(flat(0, 8));
    await four;
    await land.frames(2);
    const refogged = await look();
    return {
      fresh,
      opened,
      emptied,
      cleared,
      replaced,
      waiting,
      coarse,
      fine,
      pending,
      failed,
      partial,
      retrying,
      retried,
      refused,
      stale,
      own,
      refogged,
    };
  });
  const tone = { drawn: 512 * 512, href: false };
  const whole = { drawn: 512 * 512, href: true };
  expect(seen).toMatchObject({
    fresh: 0,
    opened: 0,
    emptied: [0, 0],
    cleared: [0, 0],
    replaced: 1,
    waiting: { ...tone, raster: "pending" },
    coarse: { ...whole, raster: "coarse", samples: "8" },
    fine: { ...whole, raster: "ready", samples: "16" },
    pending: { ...tone, raster: "pending" },
    failed: { ...tone, raster: "failed" },
    retrying: { ...tone, raster: "pending" },
    retried: { ...whole, raster: "ready", samples: "8" },
    refused: { ...whole, raster: "coarse", samples: "8" },
    stale: { ...tone, raster: "pending" },
    own: { ...whole, raster: "ready", samples: "32" },
    refogged: { ...whole, raster: "ready", samples: "8" },
  });
  // One flat tone, whichever map was seen before it.
  for (const name of ["pending", "failed", "retrying", "stale"] as const) {
    expect(seen[name].middle, name).toEqual(seen.waiting.middle);
    expect(seen[name].right, name).toEqual(seen.waiting.middle);
  }
  const { left, middle: remembered, right: inSight } = seen.partial;
  expect(left[3], "unexplored").toBe(0);
  expect(inSight).toEqual(seen.waiting.middle);
  expect(luminance(remembered)).toBeLessThan(luminance(inSight));
  for (const flat of [remembered, inSight]) {
    expect(flat[3]).toBe(255);
    expect(contrast(flat, chart.centre)).toBeGreaterThanOrEqual(1.3);
    expect(contrast(flat, chart.edge)).toBeGreaterThanOrEqual(1.3);
  }
  // Setting S of visible, level ocean, grass, forest, lake and grass.
  for (const [name, colour] of [
    ["coarse", [28, 84, 132]],
    ["fine", [132, 172, 80]],
    ["retried", [35, 86, 48]],
    ["refused", [35, 86, 48]],
    ["own", [50, 125, 182]],
    ["refogged", [132, 172, 80]],
  ] as const) {
    expect(apart(seen[name].middle, colour), name).toBeLessThanOrEqual(2);
  }
});

test("land is drawn with its picture: not if it fails, nor once the map is gone", async ({
  page,
}) => {
  await landPage(page);
  const seen = await page.evaluate(async () => {
    const land = Reflect.get(window, "land") as Land;
    const panel = document.getElementById("minimap");
    const image = document.getElementById("minimap-terrain");
    if (!panel || !image) throw new Error("The minimap is missing");
    let retry: () => void = () => undefined;
    const later = window.setTimeout.bind(window);
    Reflect.set(window, "setTimeout", (run: () => void, delay?: number) => {
      if (delay !== 5000) return later(run, delay);
      retry = run;
      return 0;
    });
    const flat = (kind: number) => land.raster(8, () => kind | (8 << 4));
    // Acts once the next picture is asked for, before it can load.
    const picture = (act: () => void) => {
      const watch = new MutationObserver(() => {
        if (!image.hasAttribute("href")) return;
        watch.disconnect();
        act();
      });
      watch.observe(image, { attributeFilter: ["href"] });
    };
    const fail = () => {
      image.dispatchEvent(new Event("error"));
    };
    const look = async () => {
      land.fog(() => 2);
      await land.frames(3);
      const { data, width } = land.composed();
      const from = (256 * width + 256) * 4;
      return {
        raster: panel.dataset["raster"],
        colour: Array.from(data.slice(from, from + 4)),
      };
    };

    await land.open({});
    const none = await look();

    picture(fail);
    await land.open({ fine: flat(0) });
    const failed = await look();
    retry();
    await land.until("ready");
    const retried = await look();

    // A refinement whose picture fails leaves the coarse land; the retry asks
    // for the refinement alone.
    const late = land.hold();
    const one = land.open({ coarse: flat(2), fine: late.reply });
    await land.until("coarse");
    picture(fail);
    late.release(flat(11));
    await one;
    const kept = await look();
    retry();
    await land.until("ready");
    const refined = await look();

    const canvas = HTMLCanvasElement.prototype;
    const encode: unknown = Reflect.get(canvas, "toBlob");
    Reflect.set(canvas, "toBlob", (done: (blob: null) => void) => done(null));
    await land.open({ fine: flat(0) });
    Reflect.set(canvas, "toBlob", encode);
    const unencoded = await look();

    // The map changes in the moment its picture has loaded.
    const next = land.hold();
    let two: Promise<void> = Promise.resolve();
    picture(() => {
      image.dispatchEvent(new Event("load"));
      two = land.open({ fine: next.reply });
    });
    await land.open({ fine: flat(0) });
    const gone = await look();
    next.release(flat(10));
    await two;
    const own = await look();
    return {
      none,
      failed,
      retried,
      kept,
      refined,
      unencoded,
      gone,
      own,
    };
  });
  const tone = seen.none.colour;
  expect(tone[3]).toBe(255);
  for (const [name, raster, colour] of [
    ["none", "failed", tone],
    ["failed", "failed", tone],
    ["retried", "ready", [132, 172, 80]],
    ["kept", "coarse", [35, 86, 48]],
    ["refined", "ready", [28, 84, 132]],
    ["unencoded", "failed", tone],
    ["gone", "pending", tone],
    ["own", "ready", [50, 125, 182]],
  ] as const) {
    expect.soft(seen[name].raster, name).toBe(raster);
    expect.soft(apart(seen[name].colour, colour), name).toBeLessThanOrEqual(2);
  }
});
