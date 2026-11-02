import type { Page } from "@playwright/test";
import { scoutCombat, villagerCombat } from "../../wire/combat.js";
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

// Current protocol, 16,384-unit client workload, distributed over the
// full 16k world. This is a synthetic client workload, not a live RTS claim.
export async function populationFixture(page: Page) {
  const count = 16384;
  const units = Buffer.from(
    Array.from({ length: count }, (_, id) => {
      const x = id ? (id % 128) * 128 + 64.5 : 8192.5;
      const y = id ? Math.floor(id / 128) * 128 + 64.5 : 8192.5;
      return [
        ...uint(id),
        0,
        ...uint(x * 2048),
        ...uint(y * 2048),
        0,
        0,
        0,
        id ? 0 : 1,
        0,
        0,
        0,
        ...(id ? villagerCombat : scoutCombat),
      ];
    }).flat(),
  );
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
      socket.send(
        Buffer.concat([
          Buffer.from([1, ...revision, 0, ...uint(count)]),
          units,
          Buffer.from([0, 0]),
        ]),
      );
      socket.send(disabledFog(revision));
    });
  });
  return count;
}
