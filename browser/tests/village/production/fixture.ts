import { type Page, type WebSocketRoute } from "@playwright/test";
import { inputFixture, sint, uint } from "../input/fixture.js";

export type ProductionBuilding = {
  id: number;
  kind: number;
  x: number;
  y: number;
  player?: number;
  complete?: boolean;
  queue?: number[];
};
export function productionBuilding(b: ProductionBuilding) {
  const side = b.kind === 0 ? 4 : b.kind === 2 ? 2 : 3;
  const queue = b.queue ?? [];
  return [
    b.id,
    b.player ?? 0,
    b.kind,
    ...sint(b.x),
    ...sint(b.y),
    ...sint(b.x + side),
    ...sint(b.y + side),
    b.complete === false ? 0 : 1,
    ...uint(b.complete === false ? 200 : 1000),
    ...uint(1000),
    ...uint(1000),
    8,
    0,
    queue.length,
    ...queue.flatMap((kind) => [kind, 0, ...uint(500)]),
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    // No bridge span.
    0,
  ];
}
export function playerState(
  age: number,
  researched = 0,
  population = 4,
  cap = 5,
) {
  return [
    0,
    ...uint(10000),
    ...uint(10000),
    ...uint(10000),
    ...uint(10000),
    population,
    cap,
    age,
    ...uint(researched),
    100,
    100,
    100,
  ];
}

/** Wire fixture: real WASM picking, command dispatch and DOM. Simulation tests
 * separately exercise authoritative acceptance, costs and completion. */
export async function productionFixture(
  page: Page,
  buildings: ProductionBuilding[],
  age = 0,
  researched = 0,
) {
  let revision: number[] = [1],
    socket: WebSocketRoute | undefined,
    tick = 0;
  const player = playerState(age, researched);
  const encoded = buildings.map(productionBuilding);
  const update = (nextPlayer = player) => {
    if (nextPlayer !== player) player.splice(0, player.length, ...nextPlayer);
    socket?.send(
      Buffer.from([
        2,
        ...revision,
        ...uint(++tick),
        0,
        0,
        1,
        ...player,
        buildings.length,
        ...buildings.flatMap(productionBuilding),
        0,
      ]),
    );
  };
  const commands = await inputFixture(
    page,
    "webgpu",
    encoded,
    false,
    0,
    (bytes, route) => {
      socket = route;
      if (bytes[0] === 1) {
        buildings.forEach((building, i) => {
          encoded[i] = productionBuilding(building);
        });
        revision = [];
        for (const byte of bytes.subarray(1)) {
          revision.push(byte);
          if (byte < 128) break;
        }
      }
      if (bytes[0] === 5) {
        const building = buildings.find((b) => b.id === bytes[2]);
        if (building) {
          building.queue ??= [];
          building.queue.push(bytes[3] ?? 0);
          route.send(Buffer.from([3, bytes[1] ?? 0, 0, ...uint(tick)]));
          update();
        }
      }
    },
    { player },
  );
  return { commands, update };
}
