import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { postcardFields, scoutCombat } from "../wire/combat.js";
import { disabledFog } from "../wire/fog.js";

test.use({ video: "on" });

function unsigned(value: number): number[] {
  const bytes: number[] = [];
  do {
    const digit = value % 128;
    value = Math.floor(value / 128);
    bytes.push(digit + (value ? 128 : 0));
  } while (value);
  return bytes;
}

const cap = 65_536;
const own = 65_000;
const visible = 100;
const margin = 500;

test("an over-limit camera guard falls back to the visible subscription", async ({
  page,
}) => {
  await gameAssets(page, true);
  const requests: number[][] = [];
  await page.routeWebSocket("**/game/ws", (socket) => {
    socket.onMessage((message) => {
      if (typeof message === "string") return;
      if (message[0] === 0) {
        socket.send(
          Buffer.from([
            0,
            30,
            1,
            0,
            0,
            ...unsigned(8192),
            ...unsigned(8192),
            ...unsigned(1024),
            20,
            1,
            ...Array<number>(24).fill(1),
          ]),
        );
        return;
      }
      if (message[0] !== 1) return;
      const fields = postcardFields(message, 7);
      const [, revision = 0, left = 0, top = 0, right = 0, bottom = 0] = fields;
      requests.push(fields);
      const [, , firstLeft = 0, firstTop = 0] = requests[0] ?? [];
      // The fixture has the extra enemies one tile inside the original guard.
      const bandX = firstLeft + 2;
      const bandY = firstTop + 2;
      const coversBand =
        left <= bandX && bandX < right && top <= bandY && bandY < bottom;
      const count = own + visible + (coversBand ? margin : 0);
      if (count > cap) {
        socket.send(Buffer.from([12, ...unsigned(revision), ...unsigned(413)]));
        return;
      }
      const units = Array.from({ length: count }, (_, index) => {
        const enemy = index >= own;
        const x = enemy ? 2044 + ((index - own) % 10) : 64;
        const y = enemy ? 2044 + Math.floor((index - own) / 10) : 64;
        return Buffer.from([
          ...unsigned(index + 1),
          Number(enemy),
          ...unsigned(x * 2048 + 1024),
          ...unsigned(y * 2048 + 1024),
          0,
          0,
          4,
          1,
          0,
          0,
          0,
          ...scoutCombat,
        ]);
      });
      const snapshot = Buffer.concat([
        Buffer.from([1, ...unsigned(revision), 1, ...unsigned(count)]),
        ...units,
        Buffer.from([0, 0]),
      ]);
      socket.send(snapshot);
      socket.send(disabledFog(unsigned(revision)));
      // Same revision must also accept a subsequent live state change.
      socket.send(Buffer.from([9, ...unsigned(revision), 2, 0]));
    });
  });

  await page.goto("/?play=1#x=2048&y=2048");
  await expect(page.locator("#scene")).toHaveAttribute("data-projectiles", "0");
  expect(requests).toHaveLength(2);
  const [kind, revision = 0, x0 = 0, y0 = 0, x1 = 0, y1 = 0, zoom] =
    requests[0] ?? [];
  expect(requests[1]).toEqual([
    kind,
    revision + 1,
    x0 + 128,
    y0 + 128,
    x1 - 128,
    y1 - 128,
    zoom,
  ]);
  const count = await page.evaluate(async () => {
    const module = (await import(
      new URL("/pkg/aoe_client.js", location.href).href
    )) as { client_memory_snapshot(): string };
    return JSON.parse(module.client_memory_snapshot()).units as number;
  });
  expect(count).toBe(65_100);
  await expect(page.locator("#connection")).toHaveText("connected");
});
