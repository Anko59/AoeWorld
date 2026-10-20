import { darkAgeState } from "../../wire/buildings.js";
import type { Page } from "@playwright/test";
import { scoutCombat, villagerCombat } from "../../wire/combat.js";
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
export async function workerFixture(page: Page) {
  await page.routeWebSocket("**/game/ws", (socket) => {
    socket.onMessage((message) => {
      const bytes =
        typeof message === "string" ? Buffer.from(message) : message;
      if (bytes[0] === 0)
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
        ...(task === 2 ? [1, 2] : [0]),
        ...(kind === 1 ? scoutCombat : villagerCombat),
      ];
      socket.send(
        Buffer.from([
          1,
          ...revision,
          0,
          3,
          ...unit(0, 8192.5, 8192.5, 1, 0, [0]),
          ...unit(2, 12000.5, 12000.5, 0, 2, [0]),
          ...unit(3, 8193.5, 8192.5, 0, 3, [1, 1, 3]),
          1,
          0,
          ...uint(200),
          ...uint(200),
          100,
          ...uint(200),
          3,
          5,
          ...darkAgeState,
          0,
        ]),
      );
      socket.send(wildlifeState(revision));
      socket.send(disabledFog(revision));
    });
  });
}
