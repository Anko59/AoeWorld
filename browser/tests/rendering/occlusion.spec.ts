import { expect, type Page } from "@playwright/test";
import { scoutCombat, villagerCombat } from "../wire/combat.js";
import {
  darkAgeState,
  marketPrices,
  townCenterState,
} from "../wire/buildings.js";
import { disabledFog } from "../wire/fog.js";

import { PNG } from "pngjs";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { activateSyntheticMap } from "../synthetic-map.js";
import { forceWebGl } from "./backends.js";
import { freshHud } from "../village/fixtures/resources.js";

test("visible friendly and enemy units retain player-coloured contours behind trees", async ({
  page,
}, info) => {
  if (info.project.name !== "webgpu") await forceWebGl(page);
  await gameAssets(page, true);
  await occlusionFixture(page, true);
  await page.goto("/?play=1#x=8192&y=8192&zoom=1");
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    info.project.name === "webgpu" ? "webgpu" : "webgl2",
  );
  await activateSyntheticMap(
    page,
    (x, y) => (x === 8192 && y === 8192) || (x === 8195 && y === 8190),
  );
  await freshHud(page);
  const actors = JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as { id: number; x: number; y: number }[];
  const image = PNG.sync.read(await page.locator("#scene").screenshot());
  const count = (
    id: number,
    color: readonly number[],
    top: number,
    bottom: number,
  ) => {
    const actor = actors.find((a) => a.id === id);
    if (!actor) throw new Error(`Missing fixture actor ${id}`);
    let found = 0;
    for (let y = Math.ceil(actor.y + top); y < actor.y + bottom; y++) {
      for (let x = Math.ceil(actor.x - 18); x < actor.x + 19; x++) {
        if (
          color.every(
            (c, channel) =>
              Math.abs(
                (image.data[(y * image.width + x) * 4 + channel] ?? 0) - c,
              ) <= 2,
          )
        )
          found++;
      }
    }
    return found;
  };
  // The tree's opaque rectangle covers this whole vertical strip of each
  // actor. Ordinary blue pixels above it remain the native sprite colour.
  expect(count(0, [65, 145, 245], -20, -4)).toBeGreaterThan(20);
  expect(count(3, [245, 65, 65], -20, -4)).toBeGreaterThan(20);
  expect(count(0, [65, 145, 245], -50, -35)).toBeGreaterThan(300);
  expect(count(0, [235, 40, 200], -20, -4)).toBeGreaterThan(300);
  expect(count(3, [235, 40, 200], -20, -4)).toBeGreaterThan(300);
});

async function occlusionFixture(page: Page, trees = false, rally = false) {
  const uint = (value: number) => {
    const bytes: number[] = [];
    do {
      const low = value % 128;
      value = Math.floor(value / 128);
      bytes.push(low + (value ? 128 : 0));
    } while (value);
    return bytes;
  };
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
        player: number,
        x: number,
        y: number,
        kind: number,
      ) => [
        id,
        player,
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
      // Frozen presentation: two opposing units behind the roof, one visible
      // villager in front. Original sprites and building composition are used.
      socket.send(
        Buffer.from([
          1,
          ...revision,
          0,
          3,
          ...unit(0, 0, trees ? 8192 : 8189.5, trees ? 8192 : 8191.5, 1),
          ...unit(2, 0, trees ? 8190 : 8194.5, trees ? 8194 : 8194.5, 0),
          ...unit(3, 1, trees ? 8195 : 8190.5, trees ? 8190 : 8189.5, 0),
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
          ...(trees
            ? [0]
            : [
                1,
                1,
                0,
                0,
                ...uint(8190 * 2),
                ...uint(8190 * 2),
                ...uint(8194 * 2),
                ...uint(8194 * 2),
                ...(rally
                  ? [
                      ...townCenterState.slice(0, -3),
                      1,
                      0,
                      ...uint(8195 * 2048),
                      ...uint(8191 * 2048),
                      ...uint(8195 * 2048),
                      ...uint(8191 * 2048),
                      0, // Prepaid Mill reseeds follow the rally point.
                      0, // Current garrison list, empty in this fixture.
                    ]
                  : townCenterState),
              ]),
        ]),
      );
      socket.send(disabledFog(revision));
    });
  });
}

test("a selected production building displays its authoritative rally flag on each backend", async ({
  page,
}, info) => {
  if (info.project.name !== "webgpu") await forceWebGl(page);
  await gameAssets(page, true);
  await occlusionFixture(page, false, true);
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.goto("/?play=1#zoom=1");
  await expect(page.locator("#connection")).toHaveText("connected");
  await activateSyntheticMap(page);
  await page.keyboard.press("h");
  await freshHud(page);
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-rally-point",
    /.+/,
  );
  const clip = { x: 875, y: 360, width: 45, height: 75 };
  const selected = PNG.sync.read(await page.screenshot({ clip }));
  await page.keyboard.press("Escape");
  await freshHud(page);
  await expect(page.locator("#scene")).toHaveAttribute("data-rally-point", "");
  const deselected = PNG.sync.read(await page.screenshot({ clip }));
  let changed = 0;
  for (let at = 0; at < selected.data.length; at += 4)
    if (
      Math.abs((selected.data[at + 2] ?? 0) - (deselected.data[at + 2] ?? 0)) >
      50
    )
      changed++;
  expect(
    changed,
    "the flag must change world pixels, not only a DOM attribute",
  ).toBeGreaterThan(500);
});

test("records the browser's unforced game renderer", async ({ page }, info) => {
  await gameAssets(page, true);
  await occlusionFixture(page, true);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    /^(webgpu|webgl2|canvas2d)$/,
  );
  const observed = await page.evaluate(() => ({
    browser: navigator.userAgent,
    webgpuAPI: !!Reflect.get(navigator, "gpu"),
    renderer: document.getElementById("playground")?.dataset.renderer,
  }));
  console.log(
    `${info.project.name} default renderer: ${JSON.stringify(observed)}`,
  );
});
