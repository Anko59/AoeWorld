import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import {
  apart,
  chart,
  contrast,
  landPage,
  layouts,
  luminance,
  place,
  shot,
  type Land,
} from "./land-page.js";

const { centre, edge } = chart;

test("remembered land stands out from the chart and visible land from both", async ({
  page,
}) => {
  await landPage(page);
  const { tones, stops } = await page.evaluate(async () => {
    const land = Reflect.get(window, "land") as Land;
    // Three bands of forest, ocean and lake; half of each is in sight.
    const kind = (x: number) => (x < 22 ? 2 : x < 43 ? 11 : 10);
    await land.open({ fine: land.raster(64, (x) => kind(x) | (8 << 4)) });
    land.fog((_, y) => (y < 128 ? 1 : 2));
    await land.frames(2);
    const { data, width } = land.composed();
    const at = (x: number, y: number) =>
      Array.from(data.slice((y * width + x) * 4, (y * width + x) * 4 + 4));
    return {
      tones: [92, 260, 428].map((x) => ({
        remembered: at(x, 121),
        visible: at(x, 381),
      })),
      stops: Array.from(document.querySelectorAll("stop"), (node) =>
        getComputedStyle(node).stopColor.replaceAll(" ", ""),
      ),
    };
  });
  for (const colour of [centre, edge]) {
    expect(stops).toContain(`rgb(${colour.join()})`);
  }
  // The measured contrasts of setting S, less .02 for 8-bit colours.
  for (const [at, name, seen, inner, outer] of [
    [0, "forest", 1.38, 1.28, 1.49],
    [1, "ocean", 1.44, 1.34, 1.56],
    [2, "lake", 1.85, 1.9, 2.2],
  ] as const) {
    const { remembered = [], visible = [] } = tones[at] ?? {};
    expect([remembered[3], visible[3]], name).toEqual([255, 255]);
    expect(contrast(visible, remembered), name).toBeGreaterThan(seen);
    expect(contrast(remembered, centre), name).toBeGreaterThan(inner);
    expect(contrast(remembered, edge), name).toBeGreaterThan(outer);
    expect(luminance(visible), name).toBeGreaterThan(luminance(remembered));
  }
});

test("the edge fades inward only and the rim stays on visible land", async ({
  page,
}) => {
  await landPage(page);
  const seen = await page.evaluate(async () => {
    const land = Reflect.get(window, "land") as Land;
    await land.open({ fine: land.raster(64, () => 2 | (8 << 4)) });
    const look = async (fog: (x: number, y: number) => number) => {
      land.fog(fog);
      await land.frames(2);
      const { data, width } = land.composed();
      return (x: number, y = 257) =>
        Array.from(data.slice((y * width + x) * 4, (y * width + x) * 4 + 4));
    };
    const alphas = (pixel: (x: number) => number[], from: number, to: number) =>
      Array.from(
        { length: to - from + 1 },
        (_, at) => (pixel(from + at)[3] ?? NaN) / 255,
      );
    const half = await look((x) => (x >= 100 ? 1 : 0));
    const trail = await look((x) => (x === 100 ? 1 : 0));
    const lone = await look((x, y) => (x === 100 && y === 128 ? 1 : 0));
    const sight = await look((x) => (x >= 128 ? 2 : 1));
    const all = await look(() => 2);
    return {
      half: alphas(half, 198, 205),
      trail: alphas(trail, 199, 202),
      lone: alphas(lone, 199, 202),
      remembered: sight(100),
      outside: sight(255),
      rim: sight(257),
      within: sight(263),
      visible: sight(400),
      corner: all(0, 0),
      everywhere: all(400),
    };
  });
  for (const [found, want] of [
    [seen.half, [0, 0, 0.31, 0.63, 0.88, 1, 1, 1]],
    [seen.trail, [0, 0.69, 0.69, 0]],
    [seen.lone, [0, 0.45, 0.45, 0]],
  ] as const) {
    expect(apart(found, want)).toBeLessThan(0.02);
  }
  // Two pixels a cell: the rim is one cell wide, inside the visible land.
  const light = luminance(seen.visible);
  expect(luminance(seen.rim)).toBeGreaterThan(1.5 * light);
  expect(luminance(seen.outside)).toBeLessThan(light);
  expect(luminance(seen.remembered)).toBeLessThan(light);
  expect(apart(seen.within, seen.visible)).toBeLessThanOrEqual(1);
  // The world's edge is not the end of sight.
  expect(apart(seen.corner, seen.everywhere)).toBeLessThanOrEqual(1);
});

test("visible snow keeps its relief and its rim", async ({ page }) => {
  await landPage(page);
  const { inner, rim } = await page.evaluate(async () => {
    const land = Reflect.get(window, "land") as Land;
    // Shades 8 to 15 from top to bottom; sight ends half way across.
    const snow = (_: number, y: number) => 6 | ((8 + (y >> 3)) << 4);
    await land.open({ fine: land.raster(64, snow) });
    land.fog((x) => (x >= 128 ? 2 : 1));
    await land.frames(2);
    const { data, width } = land.composed();
    const column = (x: number) =>
      Array.from({ length: 8 }, (_, step) => {
        const from = ((step * 64 + 32) * width + x) * 4;
        return Array.from(data.slice(from, from + 3));
      });
    return { inner: column(400), rim: column(257) };
  });
  const light = inner.map((colour) => luminance(colour));
  // Only the last step may have reached white.
  for (let at = 1; at < 8; at++) {
    const rise = (light[at] ?? NaN) - (light[at - 1] ?? NaN);
    if (at < 7) expect(rise, `to shade ${8 + at}`).toBeGreaterThan(0);
    else expect(rise).toBeGreaterThanOrEqual(0);
  }
  for (const [at, colour] of rim.entries()) {
    expect(apart(colour, inner[at] ?? [])).toBeGreaterThanOrEqual(6);
  }
});

for (const layout of layouts) {
  test(`the ${layout} minimap draws land on a dark chart in a gilded frame`, async ({
    page,
  }) => {
    test.setTimeout(60_000);
    const map = await landPage(page, layout);
    const rings = () =>
      page.evaluate(async () => {
        const land = Reflect.get(window, "land") as Land;
        land.fog((x, y) => {
          const r = Math.hypot(x - 128, y - 128);
          return r < 50 ? 2 : r < 100 ? 1 : 0;
        });
        await land.frames(3);
      });
    await page.evaluate(async () => {
      const land = Reflect.get(window, "land") as Land;
      await land.open({ fine: land.raster(64, () => 8 << 4) });
      // The view's own outline stays clear of every place read below.
      land.present(235, 128);
    });
    await rings();
    const from = (x: number, y: number, cx: number) =>
      Math.hypot(x - cx, y - 128);
    const unexplored: [number, number][] = [];
    for (let at = 0; at < 26 * 26; at++) {
      const x = 24 + (at % 26) * 8;
      const y = 24 + Math.floor(at / 26) * 8;
      const clear = from(x, y, 128) > 112 && from(x, y, 235) > 24;
      if (clear) unexplored.push([x, y]);
    }
    const fogged = await shot(page);
    const colour = (x: number, y: number) => fogged.colour(place(map, [x, y]));
    expect(apart(colour(128, 128), [132, 172, 80]), "seen").toBeLessThan(6);
    expect(apart(colour(128, 203), [85, 109, 59]), "known").toBeLessThan(6);
    // Dark and brown, never black.
    const uncharted = (seen: typeof fogged) =>
      unexplored.filter((at) => {
        const [red, green, blue] = seen.colour(place(map, at));
        const sum = red + green + blue;
        return sum < 24 || red > 110 || green > red + 3 || blue > green + 3;
      });
    expect(uncharted(fogged), "chart beside land").toEqual([]);

    await page.evaluate(async () => {
      const land = Reflect.get(window, "land") as Land;
      land.navigation.fog(true, []);
      await land.frames(3);
    });
    const plain = await shot(page);
    const everywhere: [number, number][] = [
      [128, 128],
      [128, 203],
      ...unexplored,
    ];
    const other = everywhere.filter(
      (at) => apart(plain.colour(place(map, at)), [118, 150, 76]) >= 6,
    );
    expect(other, "the plain land, no chart").toEqual([]);

    // A thin gold line along each side of the world, over the land.
    const sides = [
      [0.5, 0, 1, 0.5],
      [1, 0.5, 0.5, 1],
      [0.5, 1, 0, 0.5],
      [0, 0.5, 0.5, 0],
    ] as const;
    const bareSides = (seen: typeof fogged) => {
      const bare: string[] = [];
      for (const [side, [x0, y0, x1, y1]] of sides.entries()) {
        for (const along of [0.19, 0.31, 0.44, 0.56, 0.69, 0.81]) {
          const x = map.width * (x0 + (x1 - x0) * along);
          const y = map.height * (y0 + (y1 - y0) * along);
          const dx = map.width / 2 - x;
          const dy = map.height / 2 - y;
          let gold = false;
          for (let inward = 0; inward <= 8; inward += 0.5) {
            const part = inward / Math.hypot(dx, dy);
            const [red, green, blue] = seen.colour([
              map.left + x + dx * part,
              map.top + y + dy * part,
            ]);
            if (red > green + 8 && green > blue + 10 && red > 45) gold = true;
          }
          if (!gold) bare.push(`side ${side} at ${along}`);
        }
      }
      return bare;
    };
    expect(bareSides(plain), "gold on every side").toEqual([]);
    // Zoomed in, only the diamond of the native menu still has its frame.
    await page.evaluate(async () => {
      const land = Reflect.get(window, "land") as Land;
      land.wheel(4);
      await land.frames(3);
    });
    const framed = layout === "native" ? 0 : 24;
    expect(bareSides(await shot(page))).toHaveLength(framed);

    // A map whose land fails: flat tones where explored, the chart elsewhere.
    await page.evaluate(async () => {
      const land = Reflect.get(window, "land") as Land;
      await land.open({});
    });
    await rings();
    const toned = await shot(page);
    const tone = (x: number, y: number) => toned.colour(place(map, [x, y]));
    expect(uncharted(toned), "only the chart").toEqual([]);
    const [inSight, remembered] = [tone(128, 128), tone(128, 203)] as const;
    for (const flat of [inSight, remembered]) {
      expect(contrast(flat, centre)).toBeGreaterThanOrEqual(1.3);
      expect(contrast(flat, edge)).toBeGreaterThanOrEqual(1.3);
    }
    expect(contrast(inSight, remembered)).toBeGreaterThan(1.1);
    expect(luminance(remembered)).toBeLessThan(luminance(inSight));
    expect(apart(tone(110, 128), inSight)).toBeLessThanOrEqual(2);
    // Neither land nor fog: the schematic map, without a chart or a tone.
    await page.evaluate(async () => {
      const land = Reflect.get(window, "land") as Land;
      land.navigation.fog(true, []);
      await land.frames(3);
    });
    const empty = await shot(page);
    const charted = everywhere.filter(
      (at) => apart(empty.colour(place(map, at)), [37, 60, 44]) >= 6,
    );
    expect(charted, "schematic").toEqual([]);
  });
}
