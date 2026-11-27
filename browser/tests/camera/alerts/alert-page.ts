import { expect, type Page } from "@playwright/test";
import { stillMap, type Still } from "../still-map.js";

/** A world point. */
export type Place = [number, number];
/** A whole px of the page and the world point under it. */
export type Spot = { x: number; y: number; world: Place };
/** What one alert paints, its strokes left out, in page CSS px. */
export type Star = { x: number; y: number; width: number; height: number };

/** `window.alerts`: what the minimap shows of its alerts now. */
export type Alerts = {
  /** Kept nodes that paint nothing are no stars. */
  stars(): Star[];
  /** The animations that run under that selector. */
  moving(selector: string): Animation[];
};

type Navigation = {
  /** World x and y of each new foreign hit on the player's own, of one tick. */
  hits(places: Float64Array): void;
};

// Far from every hit of these tests: a hit on the screen is no alert.
const CAMERA: Place = [100, 100];
const epoch = Date.UTC(2026, 0, 1);

/** Puts `window.alerts` on a loaded page. */
export const readAlerts = (page: Page) =>
  page.evaluate(() => {
    const painted = (root: Element) =>
      [root, ...root.querySelectorAll("*")].filter(
        (node) =>
          node instanceof SVGGeometryElement &&
          node.checkVisibility({
            checkOpacity: true,
            checkVisibilityCSS: true,
          }) &&
          node.getBBox().width > 0,
      );
    const alerts: Alerts = {
      stars: () =>
        [...document.querySelectorAll("[data-minimap-alert]")].flatMap(
          (root) => {
            // A node that is not drawn has an empty box.
            const boxes = painted(root)
              .map((node) => node.getBoundingClientRect())
              .filter((box) => box.width > 0);
            if (boxes.length === 0) return [];
            const left = Math.min(...boxes.map((box) => box.left));
            const right = Math.max(...boxes.map((box) => box.right));
            const top = Math.min(...boxes.map((box) => box.top));
            const bottom = Math.max(...boxes.map((box) => box.bottom));
            return [
              {
                x: (left + right) / 2,
                y: (top + bottom) / 2,
                width: right - left,
                height: bottom - top,
              },
            ];
          },
        ),
      moving: (selector) =>
        document
          .getAnimations()
          .filter(
            ({ effect }) =>
              effect instanceof KeyframeEffect &&
              effect.target?.closest(selector),
          ),
    };
    Reflect.set(window, "alerts", alerts);
  });

/** The still minimap under a stopped clock: only `run` lets time pass. */
export async function alertMap(
  page: Page,
  frame: "native" | "square" | "overview",
  reducedMotion: "reduce" | "no-preference" = "reduce",
) {
  await page.emulateMedia({ reducedMotion });
  await page.clock.install({ time: epoch });
  await stillMap(page, frame);
  await readAlerts(page);
  await page.clock.pauseAt(epoch + 600_000);
  await present(page);
  // The browser tells the page the map's size in a frame of its own, which the
  // stopped clock does not wait for.
  await expect(async () => {
    await present(page);
    const [drawn, wanted] = await page.evaluate(() => {
      const { map } = Reflect.get(window, "still") as Still;
      const layer = document.getElementById("minimap-alerts");
      const [x = NaN] = /scale\(([\d.]+)/
        .exec(layer?.getAttribute("transform") ?? "")
        ?.slice(1)
        .map(Number) ?? [NaN];
      return [256 / x, map.width];
    });
    expect(Math.abs((drawn ?? NaN) - (wanted ?? NaN))).toBeLessThan(1);
  }).toPass({ timeout: 10_000 });
}

/** Black land, so ivory and orange are measurable. */
export const blacken = (page: Page) =>
  page
    .locator("#minimap-frame > rect")
    .evaluate((land) => land.setAttribute("fill", "#000000"));

export const present = (page: Page, camera = CAMERA) =>
  page.evaluate(([x, y]) => {
    (Reflect.get(window, "still") as Still).present(x, y);
  }, camera);

/** Lets that time pass and draws the frame after it. */
export async function run(page: Page, ms: number, camera = CAMERA) {
  await page.clock.runFor(ms);
  await present(page, camera);
}

/** New hits, all of one tick. */
export const hit = (page: Page, ...places: Place[]) =>
  page.evaluate((places) => {
    const navigation = Reflect.get(window, "aoeNavigation") as Navigation;
    navigation.hits(Float64Array.from(places.flat()));
  }, places);

/** About that many px from the centre of the whole map. */
export const spot = (page: Page, right: number, down: number): Promise<Spot> =>
  page.evaluate(
    ([right, down]) => {
      const { map, at } = Reflect.get(window, "still") as Still;
      const left = map.left + map.width / 2;
      const top = map.top + map.height / 2;
      const x = Math.round(left + right);
      const y = Math.round(top + down);
      return { x, y, world: at(x - left, y - top) };
    },
    [right, down] as const,
  );

/** Zooms the minimap in around the camera. */
export async function zoomIn(page: Page, factor: number, camera: Place) {
  await page.evaluate((factor) => {
    (Reflect.get(window, "still") as Still).wheel(factor);
  }, factor);
  await present(page, camera);
}

/** Where the minimap zoomed by that factor around the camera shows a point. */
export const zoomed = (
  page: Page,
  world: Place,
  factor: number,
  camera: Place,
) =>
  page.evaluate(
    async ({ world, factor, camera }) => {
      const { minimapProject } = await import("/minimap.js" as string);
      const { map } = Reflect.get(window, "still") as Still;
      const [x, y] = minimapProject(world, [2000, 2000], {
        zoom: factor,
        center: camera,
      }) as Place;
      return {
        x: map.left + (x * map.width) / 256,
        y: map.top + (y * map.height) / 256,
      };
    },
    { world, factor, camera },
  );

export const stars = (page: Page) =>
  page.evaluate(() => (Reflect.get(window, "alerts") as Alerts).stars());

/** Exactly one star at each of these px of the page, and no other. */
export async function expectStars(
  page: Page,
  places: { x: number; y: number }[],
  apart = 1.5,
) {
  const seen = await stars(page);
  const off = places.map(({ x, y }) =>
    Math.min(...seen.map((star) => Math.hypot(star.x - x, star.y - y))),
  );
  expect(seen, JSON.stringify({ seen, places })).toHaveLength(places.length);
  for (const distance of off) expect(distance).toBeLessThan(apart);
}
