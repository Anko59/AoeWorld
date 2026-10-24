import type { Page } from "@playwright/test";
import { PNG } from "pngjs";
import { scoutCombat, villagerCombat } from "../../wire/combat.js";
import {
  darkAgeState,
  marketPrices,
  townCenterState,
} from "../../wire/buildings.js";
import { disabledFog } from "../../wire/fog.js";

export const colours = [
  [65, 145, 245],
  [245, 65, 65],
  [50, 190, 65],
  [245, 220, 40],
  [40, 220, 220],
  [175, 65, 220],
  [160, 160, 160],
  [245, 145, 40],
];
const positions = [
  [8192.5, 8192.5],
  [8195.5, 8192.5],
  [8189.5, 8192.5],
  [8192.5, 8189.5],
  [8192.5, 8195.5],
  [8187.5, 8192.5],
  [8192.5, 8187.5],
  [8197.5, 8192.5],
  [8192.5, 8197.5],
  [8198.5, 8189.5],
];
function uint(value: number): number[] {
  const bytes: number[] = [];
  do {
    const digit = value % 128;
    value = Math.floor(value / 128);
    bytes.push(digit + (value ? 128 : 0));
  } while (value);
  return bytes;
}

/** True player-mask pixels, with opaque blue artwork beside them as a sentinel. */
export async function playerMask(page: Page) {
  const mask = new PNG({ width: 2048, height: 2048 });
  for (let y = 1200; y < 1245; y++)
    for (let x = 1300; x < 1320; x++) {
      const at = (y * 2048 + x) * 4;
      mask.data[at] = 7;
      mask.data[at + 3] = 255;
    }
  const png = PNG.sync.write(mask);
  await page.route("**/asset-pack/fixture-mask.png", (route) =>
    route.fulfill({ contentType: "image/png", body: png }),
  );
}

export async function ownersFixture(page: Page) {
  const orders: number[] = [];
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
      if (bytes[0] !== 0 && bytes[0] !== 1) orders.push(bytes[0] ?? -1);
      if (bytes[0] !== 1) return;
      const revision: number[] = [];
      for (const byte of bytes.subarray(1)) {
        revision.push(byte);
        if (byte < 128) break;
      }
      const units = positions.flatMap(([x = 0, y = 0], id) => [
        id,
        id === 9 ? 99 : id,
        ...uint(x * 2048),
        ...uint(y * 2048),
        0,
        0,
        0,
        id === 0 ? 1 : 0,
        0,
        0,
        0,
        ...(id === 0 ? scoutCombat : villagerCombat),
      ]);
      socket.send(
        Buffer.from([
          1,
          ...revision,
          0,
          positions.length,
          ...units,
          1,
          0,
          ...uint(200),
          ...uint(200),
          100,
          ...uint(200),
          3,
          5,
          ...darkAgeState,
          ...marketPrices,
          1,
          40,
          3,
          0,
          ...uint(8186 * 2),
          ...uint(8186 * 2),
          ...uint(8190 * 2),
          ...uint(8190 * 2),
          ...townCenterState,
        ]),
      );
      socket.send(disabledFog(revision));
    });
  });
  return orders;
}
