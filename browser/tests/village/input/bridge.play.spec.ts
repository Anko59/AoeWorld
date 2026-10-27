import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { syntheticChunk } from "../../synthetic-map.js";
import { sint, uint } from "./fixture.js";
import { fiveVillagerFixture } from "../fixtures/village.js";
import { centerVillage } from "../fixtures/resources.js";

for (const queued of [false, true]) {
  test(`Feudal bridge placement preserves Shift=${queued} and rejects a lateral endpoint`, async ({
    page,
  }) => {
    await gameAssets(page, true);
    const orders: Buffer[] = [];
    let revision = 0;
    await fiveVillagerFixture(
      page,
      (bytes, socket) => {
        if (bytes[0] === 1) {
          revision = 0;
          let scale = 1;
          for (const byte of bytes.subarray(1)) {
            revision += (byte & 127) * scale;
            if (byte < 128) break;
            scale *= 128;
          }
        }
        if (bytes[0] === 24) {
          orders.push(bytes);
          // Return a completed span using the real current bridge wire layout.
          const bridge = [
            99,
            0,
            20,
            ...sint(8188),
            ...sint(8189),
            ...sint(8193),
            ...sint(8190),
            1,
            ...uint(1000),
            ...uint(1200),
            ...uint(1200),
            4,
            0,
            0,
            0,
            0,
            1,
            0,
            0,
            0,
            0, // Current bridge state includes an empty shelter list.
            1,
            ...sint(8188),
            ...sint(8189),
            ...sint(8192),
            ...sint(8189), // Exact raster endpoints.
          ];
          socket.send(
            Buffer.from([2, ...uint(revision), 1, 0, 0, 0, 1, ...bridge, 0]),
          );
        }
      },
      () => 0,
      true,
    );
    await page.goto("/?play=1#x=8192&y=8192&zoom=1");
    await expect(page.locator("#population")).toHaveText("6/10");
    // Presentation fixture: three water tiles, with land on both banks and
    // alongside the deck. Simulation connectivity has independent Rust tests.
    const chunks = [-1, 0].flatMap((y) =>
      [-1, 0].map((x) => {
        const chunk = syntheticChunk(256 + x, 256 + y, () => false);
        const bytes = Buffer.from(chunk.payload_hex, "hex");
        for (let i = 0; i < 1024; i++) {
          const tx = chunk.x * 32 + (i % 32);
          const ty = chunk.y * 32 + Math.floor(i / 32);
          if (ty === 8189 && tx >= 8189 && tx <= 8191)
            bytes.writeUInt32LE(11 | (1 << 12), 7 + i * 35 + 14);
        }
        return { ...chunk, payload_hex: bytes.toString("hex") };
      }),
    );
    await page.evaluate(async (chunkJson) => {
      const module = (await import(
        new URL("/pkg/aoe_client.js", location.href).href
      )) as {
        activate_surface_fixture(hash: string, chunks: string): void;
      };
      module.activate_surface_fixture("01".repeat(32), chunkJson);
    }, JSON.stringify(chunks));
    // Reframe on the flat fixture after replacing the real map elevation.
    await centerVillage(page);
    const worker = (
      JSON.parse(
        (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
      ) as { kind: string; x: number; y: number }[]
    ).find((a) => a.kind === "Villager");
    if (!worker) throw new Error("Fixture villager missing");
    await page.mouse.click(worker.x, worker.y);
    await page.locator("#build-menu").click();
    await expect(page.locator("#build-bridge")).toBeEnabled();
    await expect(page.locator("#build-bridge")).toBeInViewport({ ratio: 1 });
    await page.locator("#build-bridge").click();
    const size = page.viewportSize();
    if (!size) throw new Error("Viewport missing");
    const point = (x: number, y: number) => ({
      x: size.width / 2 + (x - y) * 64,
      y: size.height / 2 + (x + y - 16384) * 32,
    });
    const start = point(8188.5, 8189.5);
    await page.mouse.move(start.x, start.y);
    await page.mouse.down();
    const lateral = point(8192.5, 8188.5);
    await page.mouse.move(lateral.x, lateral.y, { steps: 5 });
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-placement-valid",
      "false",
    );
    expect(orders).toHaveLength(0);
    const end = point(8192.5, 8189.5);
    await page.mouse.move(end.x, end.y, { steps: 5 });
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-placement-valid",
      "true",
    );
    if (queued) await page.keyboard.down("Shift");
    await page.mouse.up();
    await expect.poll(() => orders.length).toBe(1);
    const endpoints = [
      ...sint(8188),
      ...sint(8189),
      ...sint(8192),
      ...sint(8189),
    ];
    const placed = orders[0];
    if (!placed) throw new Error("Bridge order missing");
    expect([...placed.subarray(-endpoints.length - 1, -1)]).toEqual(endpoints);
    expect(placed.at(-1)).toBe(queued ? 1 : 0);
    if (queued) {
      await expect(page.locator("#scene")).toHaveAttribute(
        "data-order-mode",
        "build",
      );
      await page.keyboard.up("Shift");
    }
    await expect(page.locator("#scene")).toHaveAttribute("data-order-mode", "");
    const deck = point(8190.5, 8189.5);
    await page.mouse.move(deck.x, deck.y);
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-command-cursor",
      "move",
    );
    await page.mouse.click(deck.x, deck.y);
    await expect(page.locator("#selection-name")).toHaveText("Bridge");
    await expect(page.locator("#building-hp")).toHaveText("1200 / 1200 HP");
  });
}

test("bridge construction is disabled in Dark Age", async ({ page }) => {
  await gameAssets(page, true);
  await fiveVillagerFixture(page);
  await page.goto("/?play=1#x=8192&y=8192&zoom=1");
  await expect(page.locator("#population")).toHaveText("6/10");
  const worker = (
    JSON.parse(
      (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
    ) as { kind: string; x: number; y: number }[]
  ).find((a) => a.kind === "Villager");
  if (!worker) throw new Error("Fixture villager missing");
  await page.mouse.click(worker.x, worker.y);
  await page.locator("#build-menu").click();
  await expect(page.locator("#build-bridge")).toBeDisabled();
  await expect(page.locator("#build-bridge")).toHaveAttribute(
    "title",
    /requires Feudal Age/,
  );
});

test("diagonal drag previews every bridge cell and marks an endpoint in a two-tile stream red", async ({
  page,
}) => {
  await gameAssets(page, true);
  const orders: Buffer[] = [];
  await fiveVillagerFixture(
    page,
    (bytes) => {
      if (bytes[0] === 24) orders.push(bytes);
    },
    () => 0,
    true,
  );
  await page.goto("/?play=1#x=8192&y=8192&zoom=1");
  await expect(page.locator("#population")).toHaveText("6/10");
  const chunks = [-1, 0].flatMap((y) =>
    [-1, 0].map((x) => {
      const chunk = syntheticChunk(256 + x, 256 + y, () => false);
      const bytes = Buffer.from(chunk.payload_hex, "hex");
      for (let i = 0; i < 1024; i++) {
        const tx = chunk.x * 32 + (i % 32);
        bytes.writeUInt32LE(
          tx === 8189 || tx === 8190 ? 11 | (3 << 12) : 1 << 21,
          7 + i * 35 + 14,
        );
      }
      return { ...chunk, payload_hex: bytes.toString("hex") };
    }),
  );
  await page.evaluate(async (json) => {
    const module = (await import(
      new URL("/pkg/aoe_client.js", location.href).href
    )) as {
      activate_surface_fixture(hash: string, chunks: string): void;
    };
    module.activate_surface_fixture("01".repeat(32), json);
  }, JSON.stringify(chunks));
  await centerVillage(page);
  const worker = (
    JSON.parse(
      (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
    ) as { kind: string; x: number; y: number }[]
  ).find((u) => u.kind === "Villager");
  if (!worker) throw new Error("Fixture villager missing");
  await page.mouse.click(worker.x, worker.y);
  await page.locator("#build-menu").click();
  await page.locator("#build-bridge").click();
  const size = page.viewportSize();
  if (!size) throw new Error("Viewport missing");
  const point = (x: number, y: number) => ({
    x: size.width / 2 + (x - y) * 64,
    y: size.height / 2 + (x + y - 16384) * 32,
  });
  // Keep the span above the fixture Town Center (whose footprint starts at y=8190).
  const from = point(8188.5, 8187.5);
  await page.mouse.move(from.x, from.y);
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-placement-count",
    "1",
  );
  await page.mouse.down();
  const wetEnd = point(8190.5, 8189.5);
  await page.mouse.move(wetEnd.x, wetEnd.y, { steps: 5 });
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-placement-valid",
    "false",
  );
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-placement-count",
    "3",
  );
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-placement-invalid",
    "1",
  );
  const to = point(8191.5, 8189.5);
  await page.mouse.move(to.x, to.y, { steps: 5 });
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-placement-count",
    "4",
  );
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-placement-valid",
    "true",
  );
  await page.mouse.up();
  await expect.poll(() => orders.length).toBe(1);
  const endpoints = [
    ...sint(8188),
    ...sint(8187),
    ...sint(8191),
    ...sint(8189),
  ];
  const placed = orders[0];
  if (!placed) throw new Error("Bridge order missing");
  expect([...placed.subarray(-endpoints.length - 1, -1)]).toEqual(endpoints);
});
