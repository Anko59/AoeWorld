import { expect, type Page, type WebSocketRoute } from "@playwright/test";
import { gameAssets } from "../../game-assets.js";
import {
  militiaCombat,
  scoutCombat,
  villagerCombat,
} from "../../wire/combat.js";
import {
  darkAgeState,
  feudalAgeState,
  marketPrices,
} from "../../wire/buildings.js";
import { disabledFog } from "../../wire/fog.js";
import { chunks } from "../../rendering/fixtures/hover.js";
import { freshHud } from "../fixtures/resources.js";

export function uint(value: number): number[] {
  const bytes: number[] = [];
  do {
    const digit = value % 128;
    value = Math.floor(value / 128);
    bytes.push(digit + (value ? 128 : 0));
  } while (value);
  return bytes;
}
export const sint = (value: number) =>
  uint(value < 0 ? -value * 2 - 1 : value * 2);
export const position = (x: number, y: number) => [
  ...sint(x * 1024),
  ...sint(y * 1024),
];

export function building(
  id: number,
  kind: number,
  x: number,
  y: number,
  side: number,
  hp = 900,
  complete = true,
  player = 0,
) {
  return [
    id,
    player,
    kind,
    ...sint(x),
    ...sint(y),
    ...sint(x + side),
    ...sint(y + side),
    complete ? 1 : 0,
    ...uint(complete ? 1000 : 200),
    ...uint(hp),
    ...uint(900),
    4,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0, // Prepaid Mill reseeds follow the rally point in the current wire.
    0, // Current building state appends its bounded garrison list.
    0, // Only bridges carry raster endpoints.
  ];
}

/** Frozen server state still uses the real client, picking, shaders and DOM input. */
export async function inputFixture(
  page: Page,
  renderer: string,
  buildings: number[][] = [],
  cursorCatalog = false,
  viewer: 0 | 1 = 0,
  observe?: (bytes: Buffer, socket: WebSocketRoute) => void,
  options: {
    carried?: boolean;
    query?: string;
    cargo?: number;
    player?: number[];
    military?: boolean;
  } = {},
) {
  const navy =
    options.cargo === undefined ? undefined : { cargo: options.cargo };
  const commands: number[][] = [];
  await gameAssets(page, true);
  if (cursorCatalog) {
    const art = {
      page: "fixture-color.png",
      x: 1300,
      y: 1200,
      width: 24,
      height: 24,
      page_width: 2048,
      page_height: 2048,
    };
    await page.route("**/asset-pack/interface.json", (route) =>
      route.fulfill({
        json: { commands: { attack: art, "build-cursor": art, garrison: art } },
      }),
    );
  }
  await page.routeWebSocket("**/game/ws", (socket) => {
    socket.onMessage((message) => {
      const bytes =
        typeof message === "string" ? Buffer.from(message) : message;
      observe?.(bytes, socket);
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
      if (bytes[0] !== 1) {
        if (bytes[0] !== 0) commands.push([...bytes]);
        return;
      }
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
        player = viewer,
        cargo = 0,
      ) => {
        const combat =
          kind === 1
            ? scoutCombat
            : kind === 2
              ? militiaCombat
              : villagerCombat;
        // Cargo precedes the optional Vassal IDs, stance and General state.
        return [
          id,
          player,
          ...position(x, y),
          0,
          0,
          0,
          kind,
          0,
          ...(options.carried && id === 2 ? [1, 1, 7] : [0]),
          0,
          ...combat.slice(0, -6),
          cargo,
          ...combat.slice(-5),
        ];
      };
      socket.send(
        Buffer.from([
          1,
          ...revision,
          0,
          navy ? 5 : 4,
          ...unit(0, 8192.5, 8192.5, 1),
          ...unit(2, 8194.5, 8194.5, 0),
          ...unit(3, 8196.5, 8194.5, options.military ? 2 : 0),
          ...unit(
            4,
            8189.5,
            8192.5,
            options.military ? 1 : 0,
            viewer === 0 ? 1 : 0,
          ),
          ...(navy ? unit(5, 8193.5, 8194.5, 20, viewer, navy.cargo) : []),
          1,
          ...(options.player ?? [
            viewer,
            ...uint(2000),
            ...uint(2000),
            100,
            ...uint(2000),
            3,
            ...(navy ? feudalAgeState : darkAgeState),
            ...marketPrices,
          ]),
          buildings.length,
          ...buildings.flat(),
        ]),
      );
      const animal = (
        id: number,
        kind: number,
        x: number,
        y: number,
        hp: number,
        food: number,
      ) => [id, kind, 0, hp, ...position(x, y), 0, ...uint(food), 0, 0, 0, 0];
      socket.send(
        Buffer.from([
          7,
          ...revision,
          0,
          3,
          ...animal(7, 1, 8189.5, 8195.5, 5, 140),
          ...animal(8, 2, 8190.5, 8197.5, 75, 340),
          ...animal(9, 3, 8192.5, 8198.5, 25, 0),
        ]),
      );
      socket.send(disabledFog(revision));
    });
  });
  const preference = renderer === "canvas2d" ? "canvas" : renderer;
  await page.goto(
    `/?play=1${options.query ?? ""}#x=8192.5&y=8192.5&zoom=.6&renderer=${preference}`,
  );
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    renderer,
  );
  const terrain = chunks().map((chunk) => {
    if (!navy) return chunk;
    const bytes = Buffer.from(chunk.payload_hex, "hex");
    for (let i = 0; i < 1024; i++) {
      const x = chunk.x * 32 + (i % 32);
      const y = chunk.y * 32 + Math.floor(i / 32);
      if (x === 8193 && y >= 8186 && y <= 8201)
        bytes.writeUInt32LE(11 | (3 << 12), 7 + i * 35 + 14);
    }
    return { ...chunk, payload_hex: bytes.toString("hex") };
  });
  await page.evaluate(async (payload) => {
    const game = await import(
      new URL("/pkg/aoe_client.js", location.href).href
    );
    game.activate_surface_fixture("04".repeat(32), payload);
  }, JSON.stringify(terrain));
  await page.keyboard.press("Home");
  await freshHud(page);
  return commands;
}

export function screen(x: number, y: number) {
  return { x: 640 + (x - y) * 64 * 0.6, y: 360 + (x + y - 16385) * 32 * 0.6 };
}
export async function selectWorker(page: Page, id = 2) {
  const p = id === 2 ? screen(8194.5, 8194.5) : screen(8196.5, 8194.5);
  await page.mouse.click(p.x, p.y);
  await expect(page.locator("#selection-name")).toHaveText("Villager");
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "1");
  await expect(page.locator("#selection")).toHaveAttribute(
    "data-owned",
    "true",
  );
}
