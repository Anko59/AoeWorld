import { expect, type Page } from "@playwright/test";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { test } from "../../server.js";
import { centerVillage, freshHud } from "../village/fixtures/resources.js";
import { scoutCombat, villagerCombat } from "../wire/combat.js";
import {
  darkAgeState,
  marketPrices,
  townCenterState,
} from "../wire/buildings.js";
import { disabledFog } from "../wire/fog.js";

type Config = { url: string; phase: "before" | "after" };
async function config(): Promise<Config | null> {
  return readFile("../.cache/occlusion-facing-proof.json", "utf8")
    .then((text) => JSON.parse(text) as Config)
    .catch(() => null);
}
async function open(page: Page, value: Config) {
  const url = new URL(value.url);
  expect(url.hostname).toBe("127.0.0.1");
  expect(url.port).toBe("8121");
  const health = await page.request.get(`${url}health`);
  expect(health.ok()).toBe(true);
  const assets = await page.request.get(`${url}asset-pack/manifest.json`);
  expect(assets.ok()).toBe(true);
  const manifest = (await assets.json()) as { building_data: string };
  expect(typeof manifest.building_data).toBe("string");
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.goto(`${url}?play=1#x=8192&y=8192&zoom=.8`);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    "webgpu",
  );
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-village-art",
    "native",
  );
  await freshHud(page);
  await mkdir("../.cache/occlusion-facing-shots", { recursive: true });
  await writeFile(
    `../.cache/occlusion-facing-shots/${value.phase}-build.json`,
    await health.text(),
  );
}

test("native building occlusion proof on the checkout dev server", async ({
  page,
}, info) => {
  const value = await config();
  test.skip(
    !value || info.project.name !== "webgpu",
    "Explicit local proof only",
  );
  if (!value) return;
  test.setTimeout(120_000);
  await occlusionFixture(page);
  await open(page, value);
  await page.screenshot({
    path: `../.cache/occlusion-facing-shots/occlusion-${value.phase}.png`,
  });
  await writeFile(
    `../.cache/occlusion-facing-shots/occlusion-${value.phase}.json`,
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  );
});

test("native working-facing proof on the checkout dev server", async ({
  page,
}, info) => {
  const value = await config();
  test.skip(
    !value || info.project.name !== "webgpu",
    "Explicit local proof only",
  );
  if (!value) return;
  test.setTimeout(120_000);
  expect((await page.request.post(`${value.url}maps/reset`)).status()).toBe(
    204,
  );
  await open(page, value);
  await centerVillage(page);
  const actors = JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as { kind: string; id: number; x: number; y: number }[];
  const worker =
    actors.find((a) => a.kind === "Villager" && a.y > 380) ??
    actors.find((a) => a.kind === "Villager");
  if (!worker) throw new Error("No villager in the native village");
  await page.mouse.click(worker.x, worker.y);
  await expect(page.locator("#selection-name")).toHaveText("Villager");
  await page.locator("#build-menu").click();
  await page.locator("#build-house").click();
  let placed = false;
  for (const [x, y] of [
    [960, 420],
    [920, 450],
    [320, 420],
    [400, 440],
  ]) {
    if (x === undefined || y === undefined)
      throw new Error("Placement point missing");
    await page.mouse.move(x, y);
    await freshHud(page);
    if (
      (await page.locator("#scene").getAttribute("data-placement-valid")) !==
      "true"
    )
      continue;
    await page.mouse.click(x, y);
    placed = true;
    break;
  }
  expect(placed).toBe(true);
  await expect(page.locator("#selection-task")).toHaveText("Building", {
    timeout: 60_000,
  });
  await freshHud(page);
  await page.screenshot({
    path: `../.cache/occlusion-facing-shots/facing-${value.phase}.png`,
  });
  await writeFile(
    `../.cache/occlusion-facing-shots/facing-${value.phase}.json`,
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  );
});

async function occlusionFixture(page: Page) {
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
          ...unit(0, 0, 8189.5, 8191.5, 1),
          ...unit(2, 0, 8194.5, 8194.5, 0),
          ...unit(3, 1, 8190.5, 8189.5, 0),
          1,
          0,
          ...uint(200),
          ...uint(200),
          100,
          ...uint(200),
          3,
          ...darkAgeState,
          ...marketPrices,
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
      socket.send(disabledFog(revision));
    });
  });
}
