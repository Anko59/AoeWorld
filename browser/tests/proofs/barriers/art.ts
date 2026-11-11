import { expect, type Page } from "@playwright/test";
import { playerEconomyTail, feudalAgeState } from "../../wire/buildings.js";
import { scoutCombat } from "../../wire/combat.js";
import { disabledFog } from "../../wire/fog.js";

export function uint(value: number): number[] {
  const bytes: number[] = [];
  do {
    const digit = value % 128;
    value = Math.floor(value / 128);
    bytes.push(digit + (value ? 128 : 0));
  } while (value);
  return bytes;
}

/** Frozen authoritative presentation fixture; movement is filmed separately live. */
export async function barrierArt(page: Page, url: string) {
  let frozen = true;
  const buildings: number[][] = [];
  const building = (
    kind: number,
    x: number,
    y: number,
    w = 1,
    h = 1,
    open = false,
  ) => {
    buildings.push([
      ...uint(100 + buildings.length),
      0,
      kind,
      ...[x, y, x + w, y + h].flatMap((v) => uint(v * 2)),
      1,
      ...uint(1000),
      ...uint(1800),
      ...uint(1800),
      6,
      0,
      0,
      0,
      0,
      1,
      Number(open),
      0,
      0,
      0,
      0,
    ]);
  };
  for (let i = 0; i < 5; i++) {
    building(21, 8184 + i, 8184);
    building(21, 8194, 8184 + i);
    building(21, 8185 + i, 8194 + i);
    building(21, 8195 + i, 8198 - i);
  }
  building(22, 8183, 8190, 4, 1);
  building(22, 8194, 8190, 1, 4);
  building(22, 8179, 8194, 4, 1, true);
  building(22, 8198, 8190, 1, 4, true);
  building(15, 8185, 8201, 4, 1);
  building(15, 8203, 8194, 1, 4);
  building(15, 8180, 8201, 4, 1, true);
  building(15, 8206, 8194, 1, 4, true);
  await page.routeWebSocket("**/game/ws", (socket) => {
    if (!frozen) {
      socket.connectToServer();
      return;
    }
    socket.onMessage((message) => {
      const bytes =
        typeof message === "string" ? Buffer.from(message) : message;
      if (bytes[0] === 0)
        socket.send(
          Buffer.from([
            0,
            7,
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
        Buffer.from([
          1,
          ...revision,
          0,
          1,
          0,
          0,
          ...uint(8182.5 * 2048),
          ...uint(8190.5 * 2048),
          0,
          0,
          0,
          1,
          0,
          0,
          0,
          ...scoutCombat,
          1,
          0,
          ...uint(2000),
          ...uint(2000),
          ...uint(2000),
          ...uint(2000),
          0,
          5,
          ...feudalAgeState,
          ...playerEconomyTail,
          ...uint(buildings.length),
          ...buildings.flat(),
        ]),
      );
      socket.send(disabledFog(revision));
    });
  });
  await page.goto(
    `${url}?play=1&barrier-art=1#x=8193&y=8193&zoom=.3&renderer=webgpu`,
  );
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60000,
  });
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-village-art",
    "native",
  );
  await expect
    .poll(
      async () =>
        JSON.parse(
          (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
        ).length,
    )
    .toBe(buildings.length + 1);
  return () => {
    frozen = false;
  };
}
