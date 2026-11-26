import { expect, type Page } from "@playwright/test";
import { test } from "../../../server.js";
import { pixels } from "../still-map.js";
import {
  type Alerts,
  type Spot,
  alertMap,
  blacken,
  run,
  spot,
} from "./alert-page.js";

/** A painted px, from the middle of its ping. */
type Dot = { x: number; y: number; red: number; blue: number };

const kinds = { move: 2, "attack-move": 3, patrol: 4 } as const;

/** Orders there with the right button; the client answers with that kind. */
const order = (page: Page, kind: keyof typeof kinds, { x, y }: Spot) =>
  page.evaluate(
    ({ answer, x, y }) => {
      const navigation = Reflect.get(window, "aoeNavigation") as {
        bindOrders(send: () => number): void;
      };
      for (const old of document.querySelectorAll("[data-minimap-ping]"))
        old.remove();
      navigation.bindOrders(() => answer);
      document.getElementById("minimap-map")?.dispatchEvent(
        new PointerEvent("pointerdown", {
          bubbles: true,
          cancelable: true,
          button: 2,
          clientX: x,
          clientY: y,
        }),
      );
      // Held where it starts, however long a screenshot takes.
      const { moving } = Reflect.get(window, "alerts") as Alerts;
      const motion = moving("[data-minimap-ping]");
      for (const animation of motion) animation.pause();
      return motion.length;
    },
    { answer: kinds[kind], x, y },
  );

const seek = (page: Page, time: number) =>
  page.evaluate((time) => {
    const { moving } = Reflect.get(window, "alerts") as Alerts;
    for (const animation of moving("[data-minimap-ping]"))
      animation.currentTime = time;
  }, time);

async function dots(page: Page, place: Spot): Promise<Dot[]> {
  const colour = await pixels(page);
  const seen: Dot[] = [];
  for (let x = -20; x <= 20; x++)
    for (let y = -20; y <= 20; y++) {
      const { red, green, blue } = colour(place.x + x, place.y + y);
      // Black land and its ink casing are no part of a ping.
      if (Math.max(red, green, blue) > 90) seen.push({ x, y, red, blue });
    }
  return seen;
}

const mean = (values: number[]) =>
  values.reduce((sum, value) => sum + value, 0) / values.length;
const radius = (seen: Dot[]) => mean(seen.map(({ x, y }) => Math.hypot(x, y)));
/** 1 on the north, east, south and west axes, -1 between them, 0 for a disc. */
const cross = (seen: Dot[]) =>
  mean(seen.map(({ x, y }) => Math.cos(4 * Math.atan2(y, x))));
const paleness = (seen: Dot[]) => mean(seen.map(({ blue, red }) => blue / red));
/** The share of the px of both shapes that only one of them paints. */
function different(first: Dot[], second: Dot[]) {
  const key = ({ x, y }: Dot) => `${x},${y}`;
  const one = new Set(first.map(key));
  const other = new Set(second.map(key));
  const shared = [...one].filter((dot) => other.has(dot)).length;
  return 1 - shared / (one.size + other.size - shared);
}

test("an order's ping is four closing chevrons: ivory for a move, orange and turned for an attack, another shape for a patrol", async ({
  page,
}) => {
  await alertMap(page, "native", "no-preference");
  await blacken(page);
  const place = await spot(page, 20, 10);
  const shape = async (kind: keyof typeof kinds) => {
    const motion = await order(page, kind, place);
    await expect(page.locator(`[data-minimap-ping="${kind}"]`)).toHaveCount(1);
    expect(motion, `${kind} moves`).toBeGreaterThan(0);
    await seek(page, 0);
    const open = await dots(page, place);
    await seek(page, 390);
    const closed = await dots(page, place);
    expect(open.length, kind).toBeGreaterThan(12);
    expect(closed.length, kind).toBeGreaterThan(12);
    return { open, closed };
  };
  const move = await shape("move");
  const attack = await shape("attack-move");
  const patrol = await shape("patrol");

  expect(cross(move.open), "north, east, south and west").toBeGreaterThan(0.4);
  expect(cross(attack.open), "turned by 45 degrees").toBeLessThan(-0.4);
  for (const { open, closed } of [move, attack]) {
    // The chevrons come 6 px nearer.
    expect(radius(open) - radius(closed)).toBeGreaterThan(3.5);
    expect(radius(open) - radius(closed)).toBeLessThan(9);
    // The native map is stretched; a ping is not.
    const [wide, high] = (["x", "y"] as const).map((axis) => {
      const places = open.map((dot) => dot[axis]);
      return Math.max(...places) - Math.min(...places);
    });
    expect(Math.abs((wide ?? NaN) - (high ?? NaN))).toBeLessThanOrEqual(2);
  }
  expect(paleness(move.open), "ivory").toBeGreaterThan(0.8);
  expect(paleness(attack.open), "orange").toBeLessThan(0.5);
  // A shape of its own, not only a colour, from the start to the end.
  for (const other of [move, attack])
    for (const time of ["open", "closed"] as const)
      expect(different(patrol[time], other[time])).toBeGreaterThan(0.35);
});

test("with reduced motion a ping stands still in its own shape for less than a second", async ({
  page,
}) => {
  await alertMap(page, "native", "reduce");
  await blacken(page);
  const place = await spot(page, 20, 10);
  const shapes: Dot[][] = [];
  for (const kind of ["move", "attack-move", "patrol"] as const) {
    expect(await order(page, kind, place), `${kind} stands still`).toBe(0);
    const before = await dots(page, place);
    await run(page, 800);
    const after = await dots(page, place);
    expect(before.length, kind).toBeGreaterThan(12);
    expect(different(before, after), kind).toBeLessThan(0.05);
    shapes.push(before);
    await run(page, 300);
    await expect(page.locator("[data-minimap-ping]")).toHaveCount(0);
  }
  const [move = [], attack = [], patrol = []] = shapes;
  expect(cross(move)).toBeGreaterThan(0.15);
  expect(cross(attack)).toBeLessThan(-0.15);
  expect(different(patrol, move)).toBeGreaterThan(0.35);
  expect(different(patrol, attack)).toBeGreaterThan(0.35);
});
