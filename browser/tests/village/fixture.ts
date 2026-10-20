import type { Page, WebSocketRoute } from "@playwright/test";
import { scoutCombat, villagerCombat } from "../wire/combat.js";
import { darkAgeState, townCenterState } from "../wire/buildings.js";
import { wildlifeState } from "../wire/wildlife.js";
import { fogState } from "../wire/fog.js";

function uint(value: number): number[] {
  const bytes: number[] = [];
  do {
    const digit = value % 128;
    value = Math.floor(value / 128);
    bytes.push(digit + (value ? 128 : 0));
  } while (value);
  return bytes;
}

/** Presentation fixture only: five villagers exceed the current starting cap. */
export async function fiveVillagerFixture(
  page: Page,
  observe?: (bytes: Buffer, socket: WebSocketRoute) => void,
  snapshotTick: () => number = () => 0,
) {
  await page.routeWebSocket("**/game/ws", (socket) => {
    socket.onMessage((message) => {
      const bytes =
        typeof message === "string" ? Buffer.from(message) : message;
      observe?.(bytes, socket);
      if (bytes[0] === 0) {
        socket.send(
          Buffer.from([
            0,
            2,
            1,
            0,
            0,
            ...uint(32768),
            ...uint(32768),
            ...uint(1024),
            20,
            0,
            0,
            0,
          ]),
        );
      }
      if (bytes[0] !== 1) return;
      let revision = 0,
        scale = 1;
      for (const byte of bytes.subarray(1)) {
        revision += (byte & 127) * scale;
        if (byte < 128) break;
        scale *= 128;
      }
      const unit = (id: number, x: number, y: number, kind = 0) => [
        id,
        0,
        ...uint(x * 2048),
        ...uint(y * 2048),
        0,
        0,
        0,
        kind,
        0,
        0,
        0,
        ...(kind === 1 ? scoutCombat : villagerCombat),
      ];
      const units = [
        unit(0, 8195.5, 8190.5, 1),
        ...[
          [-4, 4],
          [-3, 4],
          [-2, 4],
          [-4, 5],
          [-3, 5],
        ].map(([x = 0, y = 0], index) =>
          unit(index + 2, 8192.5 + x, 8192.5 + y),
        ),
      ].flat();
      socket.send(
        Buffer.from([
          1,
          ...uint(revision),
          ...uint(snapshotTick()),
          6,
          ...units,
          1,
          0,
          ...uint(200),
          ...uint(200),
          100,
          ...uint(200),
          6,
          10,
          ...darkAgeState,
          1,
          1,
          0,
          0,
          ...uint(8190 * 2),
          ...uint(8190 * 2),
          ...uint(8194 * 2),
          ...uint(8194 * 2),
          ...townCenterState,
        ]),
      );
      socket.send(wildlifeState(uint(revision)));
      socket.send(
        fogState(uint, revision, snapshotTick(), [0, 0, 1, 1], [], [], true),
      );
    });
  });
}
