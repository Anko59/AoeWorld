import { expect } from "@playwright/test";
import type { PNG } from "pngjs";
import { test } from "../../../server.js";
import {
  contradicted,
  differing,
  landPage,
  layouts,
  shot,
  type Land,
} from "./land-page.js";

// Both tests draw one land, then the same land with every sample that lies
// wholly in unexplored cells replaced: a byte or a pixel that changes was read
// from under the fog. A third land, whose explored samples differ instead,
// proves that the comparison would notice.

test("no byte of the land under the fog reaches the composed canvas", async ({
  page,
}) => {
  test.setTimeout(120_000);
  await landPage(page);
  const runs = await page.evaluate(async () => {
    const land = Reflect.get(window, "land") as Land;
    let seed = 7;
    const random = (below: number) => {
      seed = (Math.imul(seed, 1103515245) + 12345) & 0x7fffffff;
      return (seed >>> 8) % below;
    };
    const cell = () => random(14) | (random(16) << 4);
    const other = (value: number) => {
      let next = cell();
      while (next === value) next = cell();
      return next;
    };
    const near = (dx: number, dy: number, r: number) => Math.hypot(dx, dy) < r;
    const fogs: Record<string, (x: number, y: number) => number> = {
      "lone cells"(x, y) {
        return x % 9 === 4 && y % 7 === 3 ? 1 + ((x + y) & 1) : 0;
      },
      trails(x, y) {
        if (near(x - 40, y - 201, 3)) return 2;
        return x === 40 || y === 201 || x - y === 20 || x + y === 330 ? 1 : 0;
      },
      discs(x, y) {
        if (near(x - 110, y - 140, 30)) return 2;
        return near(x - 110, y - 140, 64) || near(x, y, 50) ? 1 : 0;
      },
    };
    const runs = [];
    for (const samples of [512, 300, 256, 64]) {
      for (const [name, fog] of Object.entries(fogs)) {
        const state = new Uint8Array(65536).map((_, at) =>
          fog(at & 255, at >> 8),
        );
        const first = (at: number) => Math.floor((at * 256) / samples);
        const last = (at: number) => Math.ceil(((at + 1) * 256) / samples) - 1;
        const hidden = new Uint8Array(samples * samples).map((_, at) => {
          const sx = at % samples;
          const sy = (at - sx) / samples;
          for (let y = first(sy); y <= last(sy); y++)
            for (let x = first(sx); x <= last(sx); x++)
              if (state[y * 256 + x]) return 0;
          return 1;
        });
        const draw = async (cells: Uint8Array) => {
          const cellAt = (x: number, y: number) => cells[y * samples + x] ?? 0;
          await land.open({ fine: land.raster(samples, cellAt) });
          land.fog((x, y) => state[y * 256 + x] ?? 0);
          await land.frames(2);
          return land.composed();
        };
        const truth = hidden.map(() => cell());
        const a = await draw(truth);
        const b = await draw(
          truth.map((value, at) => (hidden[at] ? other(value) : value)),
        );
        const c = await draw(
          truth.map((value, at) => (hidden[at] ? value : other(value))),
        );
        let leaked = 0;
        let inked = 0;
        let blank = 0;
        let noticed = 0;
        for (let at = 0; at < a.data.length; at++) {
          const px = (at >> 2) % 512;
          const py = at >> 11;
          const explored = state[(py >> 1) * 256 + (px >> 1)];
          if (a.data[at] !== b.data[at]) leaked++;
          if (a.data[at] !== c.data[at]) noticed++;
          if (!explored && (a.data[at] || b.data[at])) inked++;
          if (explored && at % 4 === 3 && !a.data[at]) blank++;
        }
        runs.push({
          run: `${samples} samples, ${name}`,
          size: [a.width, a.height],
          hidden: hidden.includes(1),
          leaked,
          inked,
          blank,
          noticed: noticed > 0,
        });
      }
    }
    return runs;
  });
  expect(runs).toHaveLength(12);
  for (const { run, ...found } of runs) {
    expect(found, run).toEqual({
      size: [512, 512],
      hidden: true,
      leaked: 0,
      inked: 0,
      blank: 0,
      noticed: true,
    });
  }
});

for (const layout of layouts) {
  test(`the ${layout} minimap shows no pixel of the land under the fog`, async ({
    page,
  }) => {
    test.setTimeout(90_000);
    await landPage(page, layout);
    const draw = async (kind: "truth" | "under" | "over") => {
      await page.evaluate(async (kind) => {
        const land = Reflect.get(window, "land") as Land;
        const noise = (x: number, y: number, salt: number) => {
          const low = Math.imul(x + salt * 1009, 374761393);
          const mixed = low ^ Math.imul(y + 977, 668265263);
          const hash = Math.imul(mixed ^ (mixed >>> 13), 1274126177) >>> 8;
          return (hash % 14) | (((hash >>> 8) % 16) << 4);
        };
        const fog = (x: number, y: number) => {
          const r = Math.hypot(x - 128, y - 128);
          if (r < 40) return r < 18 ? 2 : 1;
          if (x % 23 === 5 && y % 19 === 7) return 2;
          return x === 60 || y === 190 || x - y === 70 ? 1 : 0;
        };
        const raster = land.raster(512, (x, y) => {
          const changed = fog(x >> 1, y >> 1) === 0 ? "under" : "over";
          return noise(x, y, kind === changed ? 2 : 1);
        });
        await land.open({ fine: raster });
        land.fog(fog);
        land.present(165, 128);
        await land.frames(3);
      }, kind);
      const whole = await shot(page);
      await page.evaluate(async () => {
        const land = Reflect.get(window, "land") as Land;
        land.wheel(5);
        await land.frames(3);
      });
      const close = await shot(page);
      return { whole: whole.image, close: close.image };
    };
    const twice = async (kind: "truth" | "under" | "over") => {
      const first = await draw(kind);
      const second = await draw(kind);
      return (zoom: "whole" | "close"): [PNG, PNG] => [
        first[zoom],
        second[zoom],
      ];
    };
    const truth = await twice("truth");
    const under = await twice("under");
    const over = await twice("over");
    for (const zoom of ["whole", "close"] as const) {
      // The same land twice: a stray pixel at most, or nothing can be compared.
      for (const scene of [truth, under, over])
        expect(differing(...scene(zoom)), zoom).toBeLessThanOrEqual(4);
      expect(contradicted(truth(zoom), over(zoom)), zoom).toBeGreaterThan(50);
      // Scaling blends neighbours: hidden land must not be among them.
      expect(contradicted(truth(zoom), under(zoom)), zoom).toBe(0);
    }
  });
}
