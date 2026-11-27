import { expect, type Page } from "@playwright/test";
import { test } from "../../../server.js";
import type { Still } from "../still-map.js";
import {
  type Place,
  alertMap,
  expectStars,
  hit,
  present,
  run,
  spot,
  stars,
  zoomIn,
  zoomed,
} from "./alert-page.js";

test("a hit is one star at its place for eight seconds, at any minimap zoom", async ({
  page,
}) => {
  await alertMap(page, "native");
  const place = await spot(page, 40, 10);
  await hit(page, place.world);
  await run(page, 50);
  await expectStars(page, [place]);
  const [whole] = await stars(page);

  const [x, y] = place.world;
  const camera: Place = [x - 60, y + 30];
  await zoomIn(page, 8, camera);
  await run(page, 7_500, camera);
  await expectStars(page, [await zoomed(page, place.world, 8, camera)]);
  const [close] = await stars(page);
  expect(close?.width).toBeCloseTo(whole?.width ?? NaN, 1);
  expect(close?.height).toBeCloseTo(whole?.height ?? NaN, 1);
  await run(page, 700, camera);
  await expectStars(page, []);
});

test("one battle is one star: hits near it in place and time only keep it", async ({
  page,
}) => {
  await alertMap(page, "native");
  const { world } = await spot(page, 0, 20);
  const [x, y] = world;
  const camera: Place = [x - 40, y + 30];
  // There, 16 tiles are 12 px: a star that follows the blows would be seen.
  await zoomIn(page, 8, camera);
  const place = await zoomed(page, world, 8, camera);
  await hit(page, world, [x + 6, y - 4], [x - 9, y + 3]);
  await run(page, 50, camera);
  await expectStars(page, [place]);
  for (let blow = 0; blow < 6; blow++) {
    await run(page, 2_000, camera);
    await hit(page, [x + (blow % 2 ? 16 : -16), y]);
    await run(page, 50, camera);
    await expectStars(page, [place]);
  }

  // Beyond 24 tiles of the star and of every blow.
  const second: Place = [x + 60, y];
  await hit(page, second);
  await run(page, 50, camera);
  const both = [place, await zoomed(page, second, 8, camera)];
  await expectStars(page, both);
  // The last blow was 12 s after the first: it alone keeps the first star.
  await run(page, 7_600, camera);
  await expectStars(page, both);
  await run(page, 600, camera);
  await expectStars(page, []);
});

test("a fifth star takes the place of the oldest and a new map has none", async ({
  page,
}) => {
  await alertMap(page, "native");
  const places = [];
  for (const right of [-80, -40, 0, 40, 80]) {
    const place = await spot(page, right, 0);
    places.push(place);
    await hit(page, place.world);
    await run(page, 200);
  }
  await expectStars(page, places.slice(1));
  await page.evaluate(() => {
    const navigation = Reflect.get(window, "aoeNavigation") as {
      map(hash: Uint8Array): Promise<void>;
    };
    void navigation.map(new Uint8Array(32).fill(7));
  });
  await run(page, 50);
  await expectStars(page, []);
});

/** The minimap zoomed 8 times around a place and the px of that place. */
async function closeUp(page: Page, [x, y]: Place) {
  const camera: Place = [x - 40, y + 30];
  await zoomIn(page, 8, camera);
  return { camera, at: (place: Place) => zoomed(page, place, 8, camera) };
}

/** The world point that Space asks the camera for. */
async function spaceTarget(page: Page): Promise<Place> {
  await page.keyboard.press("Space");
  const hash = await page.evaluate(() => location.hash.slice(1));
  const target = new URLSearchParams(hash);
  return [Number(target.get("x")), Number(target.get("y"))];
}

test("a battle that is over never swallows a new attack, however late its timer is", async ({
  page,
}) => {
  await alertMap(page, "native");
  const { world } = await spot(page, 0, 20);
  const { camera, at } = await closeUp(page, world);
  await hit(page, world);
  await run(page, 50, camera);
  await expectStars(page, [await at(world)]);
  // A throttled page: eleven seconds pass and no timer runs.
  await page.evaluate(() => {
    for (const clock of [performance, Date]) {
      const now = clock.now.bind(clock);
      Reflect.set(clock, "now", () => now() + 11_000);
    }
  });
  const fresh: Place = [world[0] + 16, world[1]];
  await hit(page, fresh);
  await present(page, camera);
  await expectStars(page, [await at(fresh)]);
  expect(await spaceTarget(page)).toEqual(fresh);
});

test("among very many battles the stalest one without a star makes room for the newest", async ({
  page,
}) => {
  await alertMap(page, "native");
  // Sixty-four of them, 60 tiles apart.
  const battles = Array.from({ length: 64 }, (_, index): Place => [
    400 + 60 * (index % 8),
    400 + 60 * Math.floor(index / 8),
  ]);
  const [first, second] = battles;
  if (!first || !second) throw new Error("No battles");
  await hit(page, ...battles);
  await run(page, 1_000);
  await hit(page, first);
  await run(page, 1_000);

  const newest: Place = [1_500, 1_500];
  await hit(page, newest);
  await run(page, 50);
  expect(await stars(page)).toHaveLength(4);
  expect(await spaceTarget(page)).toEqual(newest);
  // The first battle was struck again, so the second was the stalest.
  await hit(page, first);
  expect(await spaceTarget(page), "still remembered").toEqual(newest);
  await hit(page, second);
  expect(await spaceTarget(page), "forgotten, so new").toEqual(second);
});

test("a left click within 6 px of a star jumps exactly to it, a right click orders on the land", async ({
  page,
}) => {
  await alertMap(page, "native");
  await page.evaluate(() => {
    const presses: [number, number, boolean][] = [];
    Reflect.set(window, "presses", presses);
    const navigation = Reflect.get(window, "aoeNavigation") as {
      bindOrders(send: (x: number, y: number, left: boolean) => number): void;
    };
    navigation.bindOrders((x, y, left) => {
      presses.push([x, y, left]);
      return left ? 0 : 1;
    });
  });
  const jump = () =>
    page.evaluate(() => (Reflect.get(window, "still") as Still).jump());
  const { x, y, world } = await spot(page, 40, 10);
  await hit(page, world);
  await run(page, 50);
  await expectStars(page, [{ x, y }]);

  for (const [across, down] of [
    [0, 0],
    [6, 0],
    [0, -5],
    [-4, 4],
  ] as const) {
    await page.mouse.click(x + across, y + down);
    expect(await jump(), `${across}, ${down} px off`).toEqual(world);
  }
  const land = await spot(page, 50, 10);
  expect(land.x - x).toBe(10);
  await page.mouse.click(land.x, land.y);
  const [right, below] = await jump();
  expect(right).toBeCloseTo(land.world[0], 3);
  expect(below).toBeCloseTo(land.world[1], 3);

  const beside = await spot(page, 46, 10);
  await page.mouse.click(beside.x, beside.y, { button: "right" });
  expect(await jump()).toEqual([NaN, NaN]);
  const last = await page.evaluate(() =>
    (Reflect.get(window, "presses") as [number, number, boolean][]).at(-1),
  );
  expect(last?.[0]).toBeCloseTo(beside.world[0], 3);
  expect(last?.[1]).toBeCloseTo(beside.world[1], 3);
  expect(last?.[2]).toBe(false);
});

test("the bars beside a zoomed whole map show stars and pings, and nothing lies beyond them", async ({
  page,
}) => {
  await alertMap(page, "overview");
  const camera: Place = [1000, 1000];
  await zoomIn(page, 8, camera);
  // In the middle of the left bar, and as far beyond its outer edge.
  const { bar, inside, beyond } = await page.evaluate(async (camera) => {
    const { minimapOrderPoint } = await import("/minimap.js" as string);
    const { map } = Reflect.get(window, "still") as Still;
    const box = document.getElementById("minimap-map")?.getBoundingClientRect();
    if (!box) throw new Error("#minimap-map is missing");
    const bar = map.left - box.left;
    const at = (x: number) => {
      const y = map.top + map.height / 2;
      const view = { zoom: 8, center: camera };
      const world = minimapOrderPoint(
        box,
        x,
        y,
        [2000, 2000],
        view,
        false,
      ) as Place | null;
      if (!world) throw new Error("No world under that px");
      return { x, y, world };
    };
    return {
      bar,
      inside: at(Math.round(box.left + bar / 2)),
      beyond: at(Math.round(box.left - bar / 2)),
    };
  }, camera);
  expect(bar, "the whole map has bars").toBeGreaterThan(40);
  const apart = inside.world.map((at, axis) => at - (beyond.world[axis] ?? at));
  expect(Math.hypot(...apart), "two battles").toBeGreaterThan(24);

  await hit(page, inside.world);
  await run(page, 50, camera);
  await expectStars(page, [inside]);
  await hit(page, beyond.world);
  await run(page, 50, camera);
  await expectStars(page, [inside]);

  await page.mouse.click(inside.x + 3, inside.y);
  const jump = await page.evaluate(() =>
    (Reflect.get(window, "still") as Still).jump(),
  );
  expect(jump[0]).toBeCloseTo(inside.world[0], 6);
  expect(jump[1]).toBeCloseTo(inside.world[1], 6);

  // An order in the bar is an order on the land there.
  const ping = await page.evaluate(({ x, y }) => {
    const navigation = Reflect.get(window, "aoeNavigation") as {
      bindOrders(send: () => number): void;
    };
    navigation.bindOrders(() => 2);
    document.getElementById("minimap-map")?.dispatchEvent(
      new PointerEvent("pointerdown", {
        bubbles: true,
        cancelable: true,
        button: 2,
        clientX: x,
        clientY: y,
      }),
    );
    const box = document
      .querySelector("[data-minimap-ping]")
      ?.getBoundingClientRect();
    if (!box) throw new Error("No ping");
    const apart = [box.x + box.width / 2 - x, box.y + box.height / 2 - y];
    return { width: box.width, apart: Math.hypot(...apart) };
  }, inside);
  expect(ping.width, "the ping is drawn").toBeGreaterThan(5);
  expect(ping.apart).toBeLessThan(2);
});

test("Space jumps to the latest alert and keeps the zoom", async ({ page }) => {
  await alertMap(page, "native");
  const hash = () => page.evaluate(() => location.hash);
  await page.keyboard.press("Space");
  expect(await hash()).toBe("");
  const first = await spot(page, -60, 0);
  const last = await spot(page, 60, 10);
  for (const place of [first, last]) {
    await hit(page, place.world);
    await run(page, 1_000);
  }
  await expectStars(page, [first, last]);
  await page.keyboard.press("Space");
  const target = new URLSearchParams((await hash()).slice(1));
  expect(Number(target.get("x"))).toBeCloseTo(last.world[0], 3);
  expect(Number(target.get("y"))).toBeCloseTo(last.world[1], 3);
  expect(Number(target.get("zoom"))).toBe(1);
});

test("'Under attack' is said politely, at most once in ten seconds", async ({
  page,
}) => {
  await alertMap(page, "native");
  await page.evaluate(() => {
    // A region that arrives together with its first words is not read out.
    const regions = [
      ...document.querySelectorAll('[aria-live="polite"],[role="status"]'),
    ];
    const said: string[] = [];
    Reflect.set(window, "said", said);
    new MutationObserver((changes) => {
      for (const { type, target, addedNodes } of changes) {
        if (!regions.some((region) => region.contains(target))) continue;
        const words =
          type === "characterData"
            ? target.textContent
            : Array.from(addedNodes, (node) => node.textContent).join(" ");
        if (words && /under attack/i.test(words)) said.push(words);
      }
    }).observe(document.body, {
      subtree: true,
      childList: true,
      characterData: true,
    });
  });
  const said = () =>
    page.evaluate(() => (Reflect.get(window, "said") as string[]).length);
  const counts = [];
  // Far apart, so each one is an alert of its own.
  for (const [right, wait] of [
    [-90, 3_000],
    [-45, 6_000],
    [0, 1_500],
    [45, 100],
  ] as const) {
    const { world } = await spot(page, right, 0);
    await hit(page, world);
    await run(page, 200);
    counts.push(await said());
    await run(page, wait);
  }
  expect(counts).toEqual([1, 1, 1, 2]);
});
