import { expect, type Page, type WebSocketRoute } from "@playwright/test";
import { gameAssets } from "../../game-assets.js";
import { position, sint, uint } from "../../village/input/fixture.js";
import {
  darkAgeState,
  playerEconomyTail,
  townCenterState,
} from "../../wire/buildings.js";
import { scoutCombat, villagerCombat } from "../../wire/combat.js";
import { disabledFog } from "../../wire/fog.js";
import { readAlerts } from "./alert-page.js";

/** Whose blow and the tick of it. */
export type Hit = readonly [attacker: number, tick: number];
/** A villager, or the scout with id 0, of the viewer unless it says so. */
export type Unit = {
  id: number;
  x: number;
  y: number;
  player?: number;
  hit?: Hit;
};
/** A Town Center of four tiles, or a palisade (7) or stone wall (21) of one.
 * `x` and `y` are its first tile. */
export type Building = Unit & { kind?: 7 | 21 };
/** A boar (2) or a wolf (3). */
export type Animal = { id: number; x: number; y: number; kind: 2 | 3 };

/** The state with its field `last_hit`, None in the shared fragments, set. */
const struck = (state: number[], at: number, hit?: Hit) =>
  hit
    ? [
        ...state.slice(0, at),
        1,
        ...uint(hit[0]),
        ...uint(hit[1]),
        ...state.slice(at + 1),
      ]
    : state;

const unit = ({ id, x, y, player = 0, hit }: Unit) => [
  ...uint(id),
  player,
  ...position(x, y),
  0,
  0,
  0,
  id === 0 ? 1 : 0,
  0,
  0,
  0,
  ...struck(id === 0 ? scoutCombat : villagerCombat, 3, hit),
];

const building = ({ id, x, y, player = 0, kind, hit }: Building) => [
  ...uint(id),
  player,
  kind ?? 0,
  ...sint(x),
  ...sint(y),
  ...sint(x + (kind ? 1 : 4)),
  ...sint(y + (kind ? 1 : 4)),
  ...struck(townCenterState, 11, hit),
];

const animal = ({ id, x, y, kind }: Animal) => [
  ...uint(id),
  kind,
  0,
  kind === 2 ? 75 : 25,
  ...position(x, y),
  0,
  ...uint(kind === 2 ? 340 : 0),
  ...[0, 0, 0, 0],
];

const list = (items: number[][]) => [...uint(items.length), ...items.flat()];

/** The real client over a server that the test plays. Its clock is at tick
 * 1,000, its rate 20 ticks a second and the viewer is player 0. */
export async function battle(
  page: Page,
  hash: string,
  units: Unit[],
  buildings: Building[] = [],
  animals: Animal[] = [],
) {
  const orders: number[] = [];
  const held = {
    units: new Map(units.map((unit) => [unit.id, unit])),
    buildings: new Map(buildings.map((building) => [building.id, building])),
  };
  let tick = 1_000;
  let views = 0;
  let revision: number[] = [];
  let socket: WebSocketRoute | undefined;
  const welcome = Buffer.from([
    ...[0, 4, 1, 0, 0],
    ...uint(32_768),
    ...uint(32_768),
    ...uint(1_024),
    ...[20, 0],
    ...Array<number>(24).fill(1),
  ]);
  const snapshot = () =>
    Buffer.from([
      1,
      ...revision,
      ...uint(tick),
      ...list([...held.units.values()].map(unit)),
      ...[1, 0, ...uint(200), ...uint(200), 100, ...uint(200), 6, 10],
      ...darkAgeState,
      ...playerEconomyTail,
      ...list([...held.buildings.values()].map(building)),
    ]);
  await gameAssets(page, true);
  await page.routeWebSocket("**/game/ws", (route) => {
    socket = route;
    route.onMessage((message) => {
      const bytes =
        typeof message === "string" ? Buffer.from(message) : message;
      orders.push(bytes[0] ?? -1);
      if (bytes[0] === 0) route.send(welcome);
      if (bytes[0] !== 1) return;
      revision = [];
      for (const byte of bytes.subarray(1)) {
        revision.push(byte);
        if (byte < 128) break;
      }
      views++;
      route.send(snapshot());
      route.send(disabledFog(revision));
      route.send(
        Buffer.from([
          7,
          ...revision,
          ...uint(tick),
          ...list(animals.map(animal)),
        ]),
      );
    });
  });
  // No tick may meet a newer subscription: the client would drop it.
  const settled = () =>
    expect(async () => {
      const before = views;
      await page.waitForTimeout(500);
      expect(before).toBeGreaterThan(0);
      expect(views).toBe(before);
    }).toPass({ timeout: 20_000 });
  await page.goto(`/?play=1#${hash}`);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
  await readAlerts(page);
  await settled();
  return {
    settled,
    /** The variant of every message of the client. */
    orders: () => orders,
    /** The number of snapshots sent so far. */
    views: () => views,
    now: () => tick,
    /** The tick that `send` will stamp. */
    next: () => tick + 1,
    /** Only the next snapshot will tell the client. */
    hold(state: Unit) {
      held.units.set(state.id, state);
    },
    /** One tick: changes and removals, each in the order of their ids. */
    send(change: {
      units?: Unit[];
      removals?: number[];
      buildings?: Building[];
      lost?: number[];
    }) {
      const { units = [], removals = [], buildings = [], lost = [] } = change;
      for (const state of units) held.units.set(state.id, state);
      for (const id of removals) held.units.delete(id);
      for (const state of buildings) held.buildings.set(state.id, state);
      for (const id of lost) held.buildings.delete(id);
      tick++;
      socket?.send(
        Buffer.from([
          2,
          ...revision,
          ...uint(tick),
          ...list(units.map(unit)),
          ...list(removals.map(uint)),
          0,
          ...list(buildings.map(building)),
          ...list(lost.map(uint)),
        ]),
      );
    },
  };
}
