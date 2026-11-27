import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { landPage, type Land } from "./land-page.js";

const MIB = 1024 ** 2;

test("the 256 mask keeps its contracts beside the composed land", async ({
  page,
}) => {
  await landPage(page);
  const seen = await page.evaluate(async () => {
    const land = Reflect.get(window, "land") as Land;
    const { fog } = land.navigation;
    const layer = document.getElementById("minimap-fog");
    const drawn = document.getElementById("minimap-known");
    if (!layer || !drawn) throw new Error("#minimap-known is missing");
    const bytes = () =>
      [layer, drawn].map((node) => Number(node.dataset["workingBytes"]));
    const mask = () => {
      const context = fog.canvas.getContext("2d");
      const data = context?.getImageData(3, 0, 3, 1).data ?? [];
      return [data[3], data[7], data[11]];
    };
    const fine = land.hold();
    void land.open({ coarse: land.raster(8, () => 8 << 4), fine: fine.reply });
    await land.until("coarse");
    const cells = [3 | (2 << 16), 4 | (1 << 16)];
    const start = fog.revision;
    fog(false, cells);
    const changed = fog.revision - start;
    fog(false, [...cells]);
    fog(false, Uint32Array.from(cells));
    const repeated = fog.revision - start;
    const known = [3, 4, 5].map((x) => fog.explored((x + 0.5) / 256, 0.001));
    const outside = [fog.explored(-0.01, 0), fog.explored(0, 1)];
    // A list longer than the mask is refused; a full one is not.
    fog(false, new Uint32Array(65537));
    const refused = { revision: fog.revision - start, mask: mask() };
    fine.release(land.raster(64, () => 8 << 4));
    await land.until("ready");
    await land.frames(2);
    const refined = fog.revision - start;
    const before = bytes();
    for (let at = 0; at < 50; at++) {
      fog(false, Uint32Array.of(at | (2 << 16)));
      await land.frames(1);
    }
    const after = bytes();
    fog(
      false,
      new Uint32Array(65536).map((_, at) => at | (1 << 16)),
    );
    const full = mask();
    const composed = land.composed();
    const painted = getComputedStyle(fog.canvas).visibility;
    fog(true, []);
    return {
      own: layer.querySelector("canvas") === fog.canvas,
      painted,
      mask: [fog.canvas.width, fog.canvas.height],
      composed: [composed.width, composed.height],
      changed,
      repeated,
      known,
      outside,
      refused,
      refined,
      full,
      bytes: [before, after],
      disabled: fog.disabled,
      hidden: getComputedStyle(layer).display,
      everywhere: fog.explored(0.9, 0.9),
    };
  });
  expect(seen).toMatchObject({
    // The mask is kept for its readers; its black must not hide the chart.
    own: true,
    painted: "hidden",
    mask: [256, 256],
    composed: [512, 512],
    changed: 1,
    repeated: 1,
    known: [true, true, false],
    outside: [false, false],
    refused: { revision: 1, mask: [0, 140, 255] },
    // The land is not part of the mask that the other readers follow.
    refined: 1,
    full: [140, 140, 140],
    disabled: true,
    hidden: "none",
    everywhere: true,
  });
  // The fog keeps its list and mask; the composed pixels come with at most as
  // much again for the land and for scratch.
  const [before = [], after = []] = seen.bytes;
  expect(after).toEqual(before);
  expect(before[0]).toBe(2 * 256 * 256 * 4);
  expect(before[1]).toBeGreaterThanOrEqual(MIB);
  expect(before[1]).toBeLessThanOrEqual(3 * MIB);
});

test("only the mask, the raster and the fog switch recompose the land", async ({
  page,
}) => {
  const map = await landPage(page);
  const counts = await page.evaluate(async () => {
    const land = Reflect.get(window, "land") as Land;
    const fine = land.hold();
    const cost: Record<string, number> = {};
    const measure = async (name: string, act: () => unknown) => {
      const before = land.paints();
      await act();
      await land.frames(4);
      cost[name] = land.paints() - before;
    };
    const coarse = land.raster(8, () => 2 | (8 << 4));
    void land.open({ coarse, fine: fine.reply });
    await land.until("coarse");
    await land.frames(2);
    const disc = (r: number) => () =>
      land.fog((x, y) => (Math.hypot(x - 128, y - 128) < r ? 2 : 1));
    const off = () => land.navigation.fog(true, []);
    await measure("mask", disc(50));
    await measure("same mask", disc(50));
    await measure("one change", disc(51));
    await measure("camera", () => {
      for (let at = 0; at < 20; at++) land.present(20 + at * 9, 128);
    });
    await measure("actors", () => {
      for (let at = 0; at < 5; at++) {
        land.navigation.actors(Float64Array.of(1, 20 + at, 20, 0, 0xff0000));
      }
    });
    await measure("zoom", () => {
      for (let at = 0; at < 5; at++) land.wheel(1.5);
    });
    await measure("overview", () => {
      const toggle = document.getElementById("overview-toggle");
      toggle?.click();
      toggle?.click();
    });
    await measure("refined", async () => {
      fine.release(land.raster(64, () => 11 | (8 << 4)));
      await land.until("ready");
    });
    await measure("fog off", off);
    await measure("still off", off);
    await measure("fog on", disc(51));
    await measure("still on", disc(51));
    // Every call differs; the frame draws the last one, and nothing before it.
    await measure("burst", () => {
      const before = land.paints();
      for (let at = 0; at < 512; at++)
        land.navigation.fog(false, Uint32Array.of(at | (2 << 16)));
      cost["during the burst"] = land.paints() - before;
    });
    const { data, width } = land.composed();
    const alpha = (x: number, y: number) => data[(y * width + x) * 4 + 3] ?? 0;
    cost["first of the burst"] = alpha(0, 0);
    cost["last of the burst"] = alpha(511, 3);
    return cost;
  });
  expect(counts["last of the burst"]).toBeGreaterThan(0);
  for (const name of ["mask", "one change", "refined", "burst"]) {
    expect(counts[name], name).toBe(1);
  }
  for (const name of ["fog off", "fog on"]) {
    expect(counts[name], name).toBeLessThanOrEqual(1);
  }
  const idle = ["same mask", "camera", "actors", "zoom", "overview"];
  const burst = ["during the burst", "first of the burst"];
  for (const name of [...idle, "still off", "still on", ...burst]) {
    expect(counts[name], name).toBe(0);
  }

  // A drag pans the minimap: only its transform may change.
  const paints = () =>
    page.evaluate(() => (Reflect.get(window, "land") as Land).paints());
  const before = await paints();
  const x = map.left + map.width / 2;
  const y = map.top + map.height / 2;
  const view = page.locator("#minimap-view");
  const still = await view.getAttribute("transform");
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + 20, y + 10, { steps: 6 });
  await page.mouse.up();
  await expect(view).not.toHaveAttribute("transform", still ?? "");
  await page.evaluate(() => (Reflect.get(window, "land") as Land).frames(4));
  expect(await paints()).toBe(before);
});
