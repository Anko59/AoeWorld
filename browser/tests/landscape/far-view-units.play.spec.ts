import { expect } from "@playwright/test";
import { test } from "../../server.js";

const MINE = 0x4191f5;
const ENEMY = 0xf54141;
const state = [8000, 8000, 500, 500, 1280, 720, 0.04, 1];
type Unit = [kind: number, x: number, y: number, id: number, colour: number];
type Pick = { x: number; y: number; count: number; hostile?: boolean };

// World tile that projects to backing pixel (sx, sy) with the state above.
const world = (sx: number, sy: number): [number, number] => {
  const a = (sx - 640) / 2.56;
  const b = (sy - 360) / 1.28;
  return [500 + (a + b) / 2, 500 + (b - a) / 2];
};
const group = (sx: number, sy: number, count: number, colour: number) =>
  Array.from({ length: count }, (): Unit => {
    const [x, y] = world(sx, sy);
    // Field 3 is the owning player; groups are binned per owner and cell.
    return [0, x, y, colour === MINE ? 1 : 2, colour];
  });

async function far(
  page: import("@playwright/test").Page,
  css: number,
  units: Unit[],
  places: number[] = [],
) {
  await page.route("**/landscape-test", (route) =>
    route.fulfill({
      contentType: "text/html",
      body: `<link rel="stylesheet" href="/interface/game.css"><canvas id="scene" style="width:${css}px;height:${(css * 720) / 1280}px"></canvas><section id="bottom-panel"><div id="commands"></div><div id="selection" data-count="0"></div></section>`,
    }),
  );
  await page.goto("/landscape-test");
  await page.evaluate(
    async ({ frame, flat, rows }) => {
      // @ts-expect-error Served browser module.
      const { initStrategic } = await import("/navigation/strategic.js");
      const s = initStrategic(
        {
          disabled: true,
          canvas: document.createElement("canvas"),
          revision: 0,
        },
        { records: () => flat, revision: () => 1 },
      );
      s.settlements(rows);
      s.frame(frame);
    },
    { frame: state, flat: units.flat(), rows: places },
  );
  const canvas = page.locator("#strategic-scene");
  await expect(canvas).toHaveAttribute("data-picks", /./);
  return async () =>
    JSON.parse((await canvas.getAttribute("data-picks")) ?? "[]") as Pick[];
}

test("an enemy force on top of my town centre stays a marked enemy group", async ({
  page,
}) => {
  // Town centre footprint around the screen centre, owned by me.
  const picks = await far(
    page,
    1280,
    [...group(640, 330, 2, MINE), ...group(640, 330, 3, ENEMY)],
    [10, 498, 498, 502, 502, 0, MINE, 1],
  );
  const found = await picks();
  const hostile = found.filter((p) => p.hostile);
  expect(
    hostile.map((p) => p.count),
    JSON.stringify(found),
  ).toEqual([3]);
});

test("a friendly and an enemy group in one 44 px cell both stay drawn", async ({
  page,
}) => {
  const picks = await far(page, 1280, [
    ...group(300, 200, 2, MINE),
    ...group(304, 204, 2, ENEMY),
  ]);
  const found = await picks();
  expect(
    found.filter((p) => p.hostile),
    JSON.stringify(found),
  ).toHaveLength(1);
  expect(found.filter((p) => !p.hostile)).toHaveLength(1);
});

test("unit group occupancy follows CSS pixels when the canvas backing differs", async ({
  page,
}) => {
  // CSS width 640 against a 1280 backing store: scale 0.5. A building sits at the
  // spot where a half-scale (wrong) check of the small group would land.
  const [bx, by] = world(418, 242);
  const picks = await far(
    page,
    640,
    [...group(800, 500, 1, MINE)],
    [10, bx - 1, by - 1, bx + 1, by + 1, 0, MINE, 1],
  );
  const found = await picks();
  expect(
    found.map((p) => p.count),
    JSON.stringify(found),
  ).toEqual([1]);
  await expect(page.locator("#strategic-scene")).toHaveAttribute(
    "data-settlements",
    "1",
  );
});
