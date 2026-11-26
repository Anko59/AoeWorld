import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { pixels, stillMap, type Still } from "./still-map.js";

test("neighbouring units leave no gap at native size and carry an outline", async ({
  page,
}) => {
  await stillMap(page, "native");
  const map = await page.evaluate(async () => {
    const still = Reflect.get(window, "still") as Still;
    // The camera stays clear of both groups.
    still.present(...still.at(0, -80));
    still.actors([
      ...still.block([-45, 0], [24, 16], {
        kind: 8,
        owner: 1,
        colour: 0x0000ff,
      }),
      ...still.block([45, 0], [24, 16], {
        kind: 0,
        owner: 2,
        colour: 0xff0000,
      }),
    ]);
    await still.frames(2);
    return still.map;
  });
  const colour = await pixels(page);
  for (const [right, own, other] of [
    [-45, "blue", "red"],
    [45, "red", "blue"],
  ] as const) {
    const x = map.left + map.width / 2 + right;
    const y = map.top + map.height / 2;
    const gaps = [];
    for (let across = -8; across <= 8; across++)
      for (let down = -4; down <= 4; down++) {
        const pixel = colour(x + across, y + down);
        if (pixel[own] < 190 || Math.max(pixel[other], pixel.green) > 90)
          gaps.push({ across, down, ...pixel });
      }
    expect(gaps, `gaps among the ${own} units`).toEqual([]);
    // White land, ink, unit: only the ink is dark in the unit's own channel.
    for (const [across, down] of [
      [1, 0],
      [-1, 0],
      [0, 1],
      [0, -1],
    ] as const) {
      const edge = Array.from(
        { length: 12 },
        (_, step) =>
          colour(x + across * (8 + step), y + down * (4 + step))[own],
      );
      expect(Math.min(...edge), `${own} outline`).toBeLessThan(215);
      expect(edge.at(-1), "the land beyond it").toBeGreaterThan(240);
    }
  }
});

test("own forces win a shared place and are drawn after their rivals", async ({
  page,
}) => {
  await stillMap(page, "native");
  const seen = await page.evaluate(async () => {
    const still = Reflect.get(window, "still") as Still;
    const own = { kind: 8, owner: 5, colour: 0x0000ff };
    const rival = { kind: 0, owner: 2, colour: 0xff0000 };
    const [x, y] = still.at(-20, 10);
    const ours = [8, x, y, own.owner, own.colour];
    const theirs = [0, x, y, rival.owner, rival.colour];
    still.present(...still.at(0, -80));
    // Own forces arrive first, are fewer and have the larger id.
    still.actors([
      ...ours,
      ...still.block([40, 10], [8, 8], own),
      ...still.block([48, 10], [8, 8], rival),
      ...[theirs, theirs, theirs, theirs].flat(),
    ]);
    await still.frames(2);
    const marks = document.getElementById("minimap-marks");
    if (!marks) throw new Error("#minimap-marks is missing");
    const order = [...marks.querySelectorAll("*")]
      .filter((node) => node instanceof SVGGeometryElement)
      .map((node) => getComputedStyle(node).fill.match(/[\d.]+/g) ?? [])
      .map(([red, , blue]) => Number(blue ?? 0) - Number(red ?? 0))
      .filter((difference) => Math.abs(difference) > 100)
      .map((difference) => (difference > 0 ? "own" : "rival"));
    const owner = document
      .querySelector('#minimap-entities rect[data-count="5"]')
      ?.getAttribute("data-player");
    return { map: still.map, order, owner };
  });
  // The counted pixel keeps its majority owner; only the drawing prefers ours.
  expect(seen.owner).toBe("2");
  expect(seen.order).toContain("own");
  expect(seen.order).toContain("rival");
  expect(seen.order.lastIndexOf("rival")).toBeLessThan(
    seen.order.indexOf("own"),
  );
  const colour = await pixels(page);
  const around = [-1, 0, 1].flatMap((across) =>
    [-1, 0, 1].map((down) =>
      colour(
        seen.map.left + seen.map.width / 2 - 20 + across,
        seen.map.top + seen.map.height / 2 + 10 + down,
      ),
    ),
  );
  expect(around.some(({ red, blue }) => blue > 150 && red < 130)).toBe(true);
  expect(around.some(({ red, blue }) => red > 150 && blue < 110)).toBe(false);
});

test("six joined buildings of one player are a town, fewer or scattered are not", async ({
  page,
}) => {
  await stillMap(page, "native");
  const seen = await page.evaluate(async () => {
    const still = Reflect.get(window, "still") as Still;
    still.present(...still.at(0, -80));
    const group = (
      count: number,
      [right, down]: [number, number],
      apart = 2,
      owner = 1,
      kind = 9,
    ) =>
      Array.from({ length: count }, (_, index) => [
        kind,
        ...still.at(
          right + (index % 3) * apart,
          down + Math.floor(index / 3) * apart,
        ),
        owner,
        0x0000ff,
      ]).flat();
    const towns = async (records: number[]) => {
      still.actors(records);
      await still.frames(1);
      const marks = document.getElementById("minimap-marks");
      if (!marks) throw new Error("#minimap-marks is missing");
      return [...marks.querySelectorAll("[data-town]")];
    };
    const sizes = async (records: number[]) =>
      (await towns(records)).map((node) => node.getAttribute("data-town"));
    const counts = {
      five: await sizes(group(5, [0, 0])),
      six: await sizes(group(6, [0, 0])),
      rival: await sizes(group(6, [0, 0], 2, 2, 1)),
      scattered: await sizes(group(6, [0, 0], 30)),
      twoPlayers: await sizes([
        ...group(3, [0, 0], 2, 1),
        ...group(3, [0, 2], 2, 2, 1),
      ]),
      units: await sizes(group(12, [0, 0], 2, 1, 8)),
      two: await sizes([...group(6, [-40, 0]), ...group(7, [40, 0])]),
      none: await sizes([]),
    };
    const keep = async (owner: number, kind: number) => {
      const [town] = await towns(group(6, [0, 0], 2, owner, kind));
      const box = town?.getBoundingClientRect();
      return [box?.width ?? 0, box?.height ?? 0];
    };
    return {
      counts: { ...counts, two: counts.two.sort() },
      size: [...(await keep(1, 9)), ...(await keep(2, 1))],
    };
  });
  expect(seen.counts).toEqual({
    five: [],
    six: ["6"],
    rival: ["6"],
    scattered: [],
    twoPlayers: [],
    units: [],
    two: ["6", "7"],
    none: [],
  });
  for (const size of seen.size) {
    expect(size).toBeGreaterThan(5);
    expect(size).toBeLessThan(14);
  }
});

test("cells stay on the land while the minimap follows a zoomed camera", async ({
  page,
}) => {
  await stillMap(page, "native");
  const map = await page.evaluate(() => {
    const still = Reflect.get(window, "still") as Still;
    const marker = document.getElementById("minimap-marker");
    if (!marker) throw new Error("#minimap-marker is missing");
    marker.style.visibility = "hidden";
    still.wheel(8);
    // Screen px at that zoom, from a camera off every grid line.
    const { width, height } = still.map;
    const near = (right: number, down: number) =>
      [
        1003.7 + 250 * (right / width + down / height),
        991.3 + 250 * (down / height - right / width),
      ] as const;
    const actor = (kind: number, right: number, down: number) => [
      kind,
      ...near(right, down),
      kind === 9 ? 1 : 2,
      kind === 9 ? 0x0000ff : 0xff0000,
    ];
    // Two groups of six buildings 8 px apart: two or three cells of 3 px,
    // so one town or two, and which of them must not change on the way.
    still.actors([
      ...Array.from({ length: 40 }, (_, at) =>
        actor(0, ((at * 37) % 120) - 59.7, ((at * 53) % 60) - 29.6),
      ).flat(),
      ...Array.from({ length: 12 }, (_, at) =>
        actor(9, (at % 2) * 8 - 4, 0),
      ).flat(),
    ]);
    Reflect.set(window, "pan", (right: number, down: number) => {
      still.present(...near(-right, -down));
      return document.querySelectorAll("#minimap-marks [data-town]").length;
    });
    return still.map;
  });
  // The land is moved by that many whole px, and nothing else is.
  const pan = (right: number, down: number) =>
    page.evaluate(
      async ([right, down]) => {
        const still = Reflect.get(window, "still") as Still;
        const pan = Reflect.get(window, "pan") as (
          right: number,
          down: number,
        ) => number;
        const towns = pan(right, down);
        await still.frames(2);
        return towns;
      },
      [right, down] as const,
    );
  const towns = [await pan(0, 0)];
  const first = await pixels(page);
  const moved = async (right: number, down: number) => {
    const next = await pixels(page);
    let marks = 0;
    let different = 0;
    for (let across = -66; across <= 66; across++)
      for (let below = -36; below <= 36; below++) {
        const x = map.left + map.width / 2 + across;
        const y = map.top + map.height / 2 + below;
        const was = first(x, y);
        const is = next(x + right, y + down);
        if (Math.min(was.red, was.green, was.blue) < 200) marks++;
        if (
          Math.max(
            Math.abs(was.red - is.red),
            Math.abs(was.green - is.green),
            Math.abs(was.blue - is.blue),
          ) > 40
        )
          different++;
      }
    expect(marks).toBeGreaterThan(100);
    return different;
  };
  await pan(12, 6);
  expect(await moved(12, 6), "after four cells by two").toBe(0);
  for (let step = 1; step <= 20; step++) towns.push(await pan(step, 0));
  expect(await moved(20, 0), "after twenty steps of one px").toBe(0);
  expect(new Set(towns).size, `towns: ${towns.join(", ")}`).toBe(1);
  expect(towns[0]).toBeGreaterThan(0);
});

test("a hundred thousand actors in one cell stay one pixel and a few shapes", async ({
  page,
}) => {
  await stillMap(page, "native");
  const seen = await page.evaluate(async () => {
    const still = Reflect.get(window, "still") as Still;
    still.present(...still.at(0, -80));
    await still.frames(2);
    const minimap = document.getElementById("minimap");
    const nodes = () => minimap?.querySelectorAll("*").length ?? 0;
    const before = nodes();
    const [x, y] = still.at(20, 10);
    const records = new Float64Array(500_000);
    for (let at = 0; at < records.length; at += 5) {
      const owner = (at / 5) % 3;
      records.set([owner ? 0 : 8, x, y, owner, 0xff << (8 * owner)], at);
    }
    still.actors(records);
    await still.frames(2);
    const cells = [...document.querySelectorAll("#minimap-entities rect")];
    return {
      added: nodes() - before,
      counts: cells.map((cell) => cell.getAttribute("data-count")),
      shapes: document.querySelectorAll("#minimap-marks path").length,
    };
  });
  expect(seen.counts).toEqual(["100000"]);
  expect(seen.shapes).toBeGreaterThan(0);
  expect(seen.added).toBeLessThanOrEqual(12);
});
