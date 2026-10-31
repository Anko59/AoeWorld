import {
  darkAgeState,
  marketPrices,
  wheelbarrowAgeState,
  fishingShipState,
} from "../../wire/buildings.js";
import type { Page } from "@playwright/test";
import {
  scoutCombat,
  villagerCombat,
  militiaCombat,
} from "../../wire/combat.js";
import { wildlifeState } from "../../wire/wildlife.js";
import { disabledFog } from "../../wire/fog.js";
function uint(value: number): number[] {
  const bytes: number[] = [];
  do {
    const digit = value % 128;
    value = Math.floor(value / 128);
    bytes.push(digit + (value ? 128 : 0));
  } while (value);
  return bytes;
}
/** Authoritative tasks independent of camera terrain and retained inventory. */
export async function workerFixture(
  page: Page,
  feudal = false,
  fishing = false,
) {
  await page.routeWebSocket("**/game/ws", (socket) => {
    socket.onMessage((message) => {
      const bytes =
        typeof message === "string" ? Buffer.from(message) : message;
      if (bytes[0] === 0)
        socket.send(
          Buffer.from([
            0,
            4,
            1,
            0,
            0,
            ...uint(32768),
            ...uint(32768),
            ...uint(1024),
            20,
            0,
            ...Array<number>(24).fill(1),
          ]),
        );
      if (bytes[0] !== 1) return;
      const revision: number[] = [];
      for (const byte of bytes.subarray(1)) {
        revision.push(byte);
        if (byte < 128) break;
      }
      const unit = (
        id: number,
        x: number,
        y: number,
        kind: number,
        task: number,
        load: number[],
      ) => [
        id,
        0,
        ...uint(x * 2048),
        ...uint(y * 2048),
        0,
        0,
        0,
        kind,
        task,
        ...load,
        ...(fishing && kind === 3 ? [1, 0] : task === 2 ? [1, 2] : [0]),
        ...(kind === 1
          ? scoutCombat
          : kind === 3
            ? fishingShipState
            : kind === 2
              ? militiaCombat
              : villagerCombat),
      ];
      socket.send(
        Buffer.from([
          1,
          ...revision,
          0,
          feudal ? 4 : 3,
          ...unit(0, 8192.5, 8192.5, feudal ? 2 : 1, 0, [0]),
          ...unit(2, 12000.5, 12000.5, 0, 2, [0]),
          ...unit(3, 8193.5, 8192.5, 0, 3, [1, 1, feudal ? 13 : 3]),
          ...(feudal
            ? unit(4, 8190.5, 8194.5, 3, fishing ? 4 : 3, [1, 0, 15])
            : []),
          1,
          0,
          ...uint(200),
          ...uint(200),
          100,
          ...uint(200),
          feudal ? 4 : 3,
          5,
          ...(feudal ? wheelbarrowAgeState : darkAgeState),
          ...marketPrices,
          0,
        ]),
      );
      socket.send(wildlifeState(revision));
      socket.send(disabledFog(revision));
    });
  });
}
