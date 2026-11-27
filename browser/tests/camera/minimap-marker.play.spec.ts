import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { pixels, stillMap, type Still } from "./still-map.js";

for (const [frame, width, height] of [
  ["native", 30, 20],
  ["square", 28, 20],
] as const)
  test(`the ${frame} marker keeps its box and strokes at every minimap zoom`, async ({
    page,
  }) => {
    await stillMap(page, frame);
    const looks = await page.evaluate(async () => {
      const still = Reflect.get(window, "still") as Still;
      const looks = [];
      for (const zoom of [1, 64]) {
        still.wheel(zoom);
        still.present(9000.25, 8000.75, 3, 16384);
        await still.frames(2);
        looks.push(still.drawn());
      }
      return looks;
    });
    const [whole, close] = looks;
    if (!whole || !close) throw new Error("Marker missing");
    for (const look of looks) {
      expect(look.kind).toBe("spot");
      // Filled casings may reach past the box; strokes are not measured.
      expect(look.width).toBeGreaterThan(width - 0.5);
      expect(look.width).toBeLessThan(width + 9);
      expect(look.height).toBeGreaterThan(height - 0.5);
      expect(look.height).toBeLessThan(height + 9);
      expect(look.strokes.length).toBeGreaterThan(0);
      for (const [across, down] of look.strokes)
        expect(Math.abs(across - down)).toBeLessThan(0.02);
    }
    expect(close.strokes).toEqual(whole.strokes);
    expect(Math.abs(close.width - whole.width)).toBeLessThan(0.5);
    expect(Math.abs(close.height - whole.height)).toBeLessThan(0.5);
  });

test("the marker is drawn over every unit and a click under it still jumps", async ({
  page,
}) => {
  await stillMap(page, "native");
  const { look, map, blocked } = await page.evaluate(async () => {
    const still = Reflect.get(window, "still") as Still;
    // Black land: a gap between the units must not pass for the marker.
    document
      .querySelector("#minimap-frame > rect")
      ?.setAttribute("fill", "#000000");
    still.present(...still.at(40, 20));
    still.actors(
      still.block([40, 20], [44, 32], { kind: 8, owner: 1, colour: 0xff00ff }),
    );
    await still.frames(2);
    const look = still.drawn();
    const blocked = [];
    for (const across of [-0.45, 0, 0.45])
      for (const down of [-0.45, 0, 0.45]) {
        const hit = document.elementFromPoint(
          look.x + across * look.width,
          look.y + down * look.height,
        );
        if (!hit?.closest("#minimap-map")) blocked.push(hit?.id ?? "nothing");
      }
    return { look, map: still.map, blocked };
  });
  expect(blocked).toEqual([]);
  const camera = [
    map.left + map.width / 2 + 40,
    map.top + map.height / 2 + 20,
  ] as const;
  const colour = await pixels(page);
  // The palest magenta, an own outline, keeps its green channel below 190.
  const ivory = (x: number, y: number, around: number) => {
    let most = 0;
    for (let across = -around; across <= around; across++)
      for (let down = -around; down <= around; down++) {
        const { red, green, blue } = colour(x + across, y + down);
        most = Math.max(most, Math.min(red, green, blue));
      }
    return most;
  };
  expect(ivory(camera[0], camera[1], 1), "the spot").toBeGreaterThan(200);
  for (const across of [-1, 1])
    for (const down of [-1, 1])
      expect(
        ivory(
          look.x + (across * look.width) / 2,
          look.y + (down * look.height) / 2,
          4,
        ),
        "a bracket",
      ).toBeGreaterThan(200);

  const x = Math.round(camera[0] + 9);
  const y = Math.round(camera[1] + 5);
  await page.mouse.click(x, y);
  const jump = await page.evaluate(
    async ([x, y]) => {
      const still = Reflect.get(window, "still") as Still;
      const { minimapWorldPoint } = await import("/minimap.js" as string);
      const view = { zoom: 1, center: [1000, 1000] };
      const want: number[] = minimapWorldPoint(
        still.map,
        x,
        y,
        [2000, 2000],
        view,
        true,
      );
      return { want, got: still.jump() };
    },
    [x, y] as const,
  );
  expect(jump.got[0]).toBeCloseTo(jump.want[0] ?? NaN, 3);
  expect(jump.got[1]).toBeCloseTo(jump.want[1] ?? NaN, 3);
});

test("a view dragged off the zoomed minimap is replaced by an arrow inside it", async ({
  page,
}) => {
  await stillMap(page, "native");
  const map = await page.evaluate(async () => {
    const still = Reflect.get(window, "still") as Still;
    still.wheel(8);
    still.present(1000, 1000);
    await still.frames(2);
    return still.map;
  });
  const x = Math.round(map.left + map.width / 2);
  const y = Math.round(map.top + map.height / 2);
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x - 300, y, { steps: 6 });
  await page.mouse.up();
  const look = await page.evaluate(async () => {
    const still = Reflect.get(window, "still") as Still;
    still.present(1000, 1000);
    await still.frames(2);
    return still.drawn();
  });
  expect(look.kind).toBe("arrow");
  expect(look.pulse, "dragging the minimap is not a jump").toBeNull();
  expect(look.width).toBeLessThan(20);
  expect(look.x - x).toBeLessThan(-100);
  expect(
    Math.abs(look.x - x) / (map.width / 2) +
      Math.abs(look.y - y) / (map.height / 2),
  ).toBeLessThan(1);
});

test("only a camera jump pulses the marker, and the pulse ends by itself", async ({
  page,
}) => {
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await stillMap(page, "native");
  const seen = await page.evaluate(async () => {
    const still = Reflect.get(window, "still") as Still;
    const show = (right: number) => {
      still.present(...still.at(right, 0));
      return still.drawn();
    };
    const calm: (string | null)[] = [];
    const quiet = async (right: number) => {
      calm.push(show(right).pulse);
      await still.frames(2);
      calm.push(still.drawn().pulse);
    };
    // The first presentation is no jump, nor is a pan of any total length.
    for (let right = -140; right <= 140; right += 20) await quiet(right);
    await quiet(116.5);
    // Zooming moves the marker over the minimap, the camera not at all.
    still.wheel(6);
    await quiet(116.5);
    still.wheel(1 / 64);
    await quiet(116.5);

    const started = performance.now();
    const place = still.map.left + still.map.width / 2 + 92;
    let look = show(92);
    const first = look.pulse;
    let widest = 0;
    let drift = 0;
    while (look.pulse !== null && performance.now() - started < 4000) {
      widest = Math.max(widest, look.width);
      drift = Math.max(drift, Math.abs(look.spot[0] - place));
      await still.frames(1);
      look = still.drawn();
    }
    const lasted = performance.now() - started;
    return { calm, first, widest, drift, lasted, last: look };
  });
  expect(seen.calm.filter((pulse) => pulse !== null)).toEqual([]);
  expect(seen.first).toBe("motion");
  expect(seen.last.pulse).toBeNull();
  expect(seen.lasted).toBeGreaterThan(200);
  expect(seen.lasted).toBeLessThan(2000);
  expect(seen.widest).toBeGreaterThan(seen.last.width * 1.3);
  expect(seen.drift, "the exact place never moves").toBeLessThan(1);
});

test("a long step onto an overlapping screen is a pan, one beyond it a jump", async ({
  page,
}) => {
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await stillMap(page, "native");
  const seen = await page.evaluate(() => {
    const still = Reflect.get(window, "still") as Still;
    const show = (right: number, down: number, zoom: number) => {
      still.present(...still.at(right, down), zoom);
      return still.drawn();
    };
    // A screen of 33.9 by 20.7 px on the whole minimap.
    show(-40, 0, 0.05);
    const steps = [show(-12, 0, 0.05), show(8, 15, 0.05)];
    // One of 108 by 66 px on a minimap zoomed 64 times; the step is 30 px.
    still.wheel(64);
    show(8, 15, 1);
    steps.push(show(8 + 30 / 64, 15, 1));
    still.wheel(1 / 4096);
    steps.push(show(8, 15, 0.05));
    return {
      kinds: steps.map(({ kind }) => kind),
      pans: steps.map(({ pulse }) => pulse),
      jump: show(48, 15, 0.05).pulse,
    };
  });
  expect(seen.kinds).toEqual(["frame", "frame", "frame", "frame"]);
  expect(seen.pans).toEqual([null, null, null, null]);
  expect(seen.jump).toBe("motion");
});

test("a pan moves a small marker without rewriting an outline", async ({
  page,
}) => {
  await stillMap(page, "native");
  const seen = await page.evaluate(async () => {
    const still = Reflect.get(window, "still") as Still;
    const root = document.getElementById("minimap-marker");
    if (!root) throw new Error("#minimap-marker is missing");
    const seen = [];
    for (const zoom of [1, 0.2]) {
      still.present(...still.at(30, 10), zoom);
      await still.frames(2);
      const observer = new MutationObserver(() => {});
      observer.observe(root, { attributes: true, subtree: true });
      // Fractions of a px: the exact place moves, the brackets snap.
      for (let step = 1; step <= 20; step++)
        still.present(...still.at(30 + step * 0.37, 10 + step * 0.23), zoom);
      const names = observer
        .takeRecords()
        .map((change) => change.attributeName);
      observer.disconnect();
      const outlines = names.filter(
        (name) => name === "d" || name === "points",
      );
      const { kind } = still.drawn();
      seen.push({ kind, outlines: outlines.length, moved: names.length > 0 });
    }
    return seen;
  });
  expect(seen).toEqual([
    { kind: "spot", outlines: 0, moved: true },
    { kind: "inset", outlines: 0, moved: true },
  ]);
});

test("reduced motion shows a second still bracket instead of a pulse", async ({
  page,
}) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await stillMap(page, "native");
  const seen = await page.evaluate(async () => {
    const still = Reflect.get(window, "still") as Still;
    still.present(...still.at(-100, 0));
    await still.frames(2);
    const rest = still.drawn();
    still.present(...still.at(100, 0));
    const started = performance.now();
    let look = still.drawn();
    const first = look.pulse;
    const widths: number[] = [];
    let running = 0;
    while (look.pulse !== null && performance.now() - started < 4000) {
      widths.push(look.width);
      running += document
        .getAnimations()
        .filter(
          ({ effect }) =>
            effect instanceof KeyframeEffect &&
            effect.target?.closest("#minimap-marker"),
        ).length;
      await still.frames(1);
      look = still.drawn();
    }
    return { rest, first, widths, running, last: look };
  });
  expect(seen.rest.pulse).toBeNull();
  expect(seen.first).toBe("still");
  expect(seen.running).toBe(0);
  expect(Math.min(...seen.widths)).toBeGreaterThan(seen.rest.width + 8);
  expect(Math.max(...seen.widths) - Math.min(...seen.widths)).toBeLessThan(0.5);
  expect(seen.last.pulse).toBeNull();
  expect(Math.abs(seen.last.width - seen.rest.width)).toBeLessThan(0.5);
});

test("a still camera writes nothing and a pan only moves what is drawn", async ({
  page,
}) => {
  await stillMap(page, "native");
  const seen = await page.evaluate(async () => {
    const still = Reflect.get(window, "still") as Still;
    const records = still.block([-60, 10], [12, 8], {
      kind: 8,
      owner: 1,
      colour: 0x0000ff,
    });
    still.present(...still.at(30, 10));
    still.actors(records);
    await still.frames(3);
    const width = still.drawn().width;

    const changes: MutationRecord[] = [];
    const observer = new MutationObserver((list) => {
      changes.push(...list);
    });
    for (const id of ["minimap", "minimap-marker"]) {
      const node = document.getElementById(id);
      if (!node) throw new Error(`#${id} is missing`);
      observer.observe(node, {
        attributes: true,
        characterData: true,
        childList: true,
        subtree: true,
      });
    }
    const take = () => {
      changes.push(...observer.takeRecords());
      return changes.splice(0);
    };
    for (let frame = 0; frame < 60; frame++) {
      still.present(...still.at(30, 10));
      still.actors(records);
      await still.frames(1);
    }
    const idle = take().length;

    let counting = false;
    let reads = 0;
    for (const [owner, name] of [
      [Element.prototype, "getBoundingClientRect"],
      [SVGGraphicsElement.prototype, "getBBox"],
      [SVGGraphicsElement.prototype, "getScreenCTM"],
    ] as const) {
      const original = Reflect.get(owner, name) as () => unknown;
      Reflect.set(owner, name, function (this: unknown) {
        if (counting) reads++;
        return original.call(this);
      });
    }
    for (let frame = 1; frame <= 30; frame++) {
      counting = true;
      still.present(...still.at(30 + frame, 10));
      counting = false;
      await still.frames(1);
    }
    const pan = take();
    return {
      width,
      idle,
      reads,
      nodes: pan.filter(({ type }) => type === "childList").length,
      moves: pan.filter(
        ({ target }) =>
          target instanceof Element && target.closest("#minimap-marker"),
      ).length,
    };
  });
  expect(seen.width).toBeGreaterThan(24);
  expect(seen.idle).toBe(0);
  expect(seen.reads, "present() reads no layout").toBe(0);
  expect(seen.nodes, "panning keeps every node").toBe(0);
  expect(seen.moves).toBeGreaterThan(0);
});
