import { expect, type WebSocketRoute } from "@playwright/test";
import { createHash } from "node:crypto";
import { PNG } from "pngjs";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { fiveVillagerFixture } from "./fixtures/village.js";
import { freshHud } from "./fixtures/resources.js";
import { darkAgeState, townCenterState } from "../wire/buildings.js";

function uint(value: number): number[] {
  const bytes: number[] = [];
  do {
    const digit = value % 128;
    value = Math.floor(value / 128);
    bytes.push(digit + (value ? 128 : 0));
  } while (value);
  return bytes;
}

test("rubble survives camera resubscription until its original game tick expiry", async ({
  page,
}) => {
  await gameAssets(page, true);
  let socket: WebSocketRoute | undefined;
  let revision: number[] = [];
  let tick = 0;
  let subscriptions = 0;
  await fiveVillagerFixture(
    page,
    (bytes, route) => {
      socket = route;
      if (bytes[0] === 1) {
        subscriptions++;
        revision = [];
        for (const byte of bytes.subarray(1)) {
          revision.push(byte);
          if (byte < 128) break;
        }
      }
    },
    () => tick,
  );
  await page.goto("/?play=1#x=8192&y=8192&zoom=1");
  await expect(page.locator("#population")).toHaveText("6/10");
  await page.mouse.move(640, 360);
  const send = (at: number, buildings: number[], removals: number[]) => {
    tick = at;
    if (!socket) throw new Error("Fixture socket missing");
    socket.send(
      Buffer.from([
        2,
        ...revision,
        ...uint(tick),
        0,
        0,
        0,
        ...buildings,
        ...removals,
      ]),
    );
  };
  // A visible House foundation, then its authoritative cancellation/removal.
  const house = [
    20,
    0,
    1,
    ...uint(8193 * 2),
    ...uint(8189 * 2),
    ...uint(8195 * 2),
    ...uint(8191 * 2),
    0,
    ...uint(500),
    ...uint(275),
    ...uint(550),
    6,
    0,
    0,
    0,
    0,
    ...darkAgeState.slice(0, 1), // Observed age, from the codec-pinned Age fragment.
    ...townCenterState.slice(-1), // Authoritative closed gate flag.
  ];
  send(1, [1, ...house], [0]);
  await expect
    .poll(async () =>
      (await page.locator("#scene").getAttribute("data-actors"))?.includes(
        '"House"',
      ),
    )
    .toBe(true);
  send(5, [0], [1, 20]);
  await expect
    .poll(async () =>
      (await page.locator("#scene").getAttribute("data-actors"))?.includes(
        '"House"',
      ),
    )
    .toBe(false);
  await freshHud(page);
  const shot = async () => {
    const image = PNG.sync.read(
      await page.screenshot({
        clip: { x: 850, y: 280, width: 180, height: 150 },
      }),
    );
    return createHash("sha256").update(image.data).digest("hex");
  };
  const rubble = await shot();
  const before = subscriptions;
  tick = 6;
  await page.mouse.move(640, 360);
  await page.mouse.down({ button: "middle" });
  await page.mouse.move(704, 360);
  await expect.poll(() => subscriptions).toBeGreaterThan(before);
  await page.mouse.move(640, 360);
  await page.mouse.up({ button: "middle" });
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-camera-x",
    "8192",
  );
  await freshHud(page);
  expect(await shot()).toEqual(rubble);
  send(64, [0], [0]);
  await freshHud(page);
  expect(await shot()).toEqual(rubble);
  send(65, [0], [0]);
  await freshHud(page);
  expect(await shot()).not.toEqual(rubble);
});
