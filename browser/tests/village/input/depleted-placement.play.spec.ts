import { expect, type WebSocketRoute } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { fiveVillagerFixture } from "../fixtures/village.js";
import { chunks } from "../../rendering/fixtures/hover.js";

const uint = (value: number): number[] => {
  const bytes: number[] = [];
  do {
    const digit = value % 128;
    value = Math.floor(value / 128);
    bytes.push(digit + (value ? 128 : 0));
  } while (value);
  return bytes;
};

test("exhaustion frees a resource tile for the build preview and order immediately", async ({
  page,
}) => {
  await gameAssets(page, true);
  let socket: WebSocketRoute | undefined;
  let subscription = 1;
  const orders: Buffer[] = [];
  await fiveVillagerFixture(page, (bytes, ws) => {
    socket = ws;
    if (bytes[0] === 1) {
      subscription = 0;
      let scale = 1;
      for (const byte of bytes.subarray(1)) {
        subscription += (byte & 127) * scale;
        if (byte < 128) break;
        scale *= 128;
      }
    }
    if (bytes[0] !== undefined && bytes[0] > 1) orders.push(bytes);
  });
  await page.goto("/?play=1#x=8192&y=8192&zoom=1");
  await expect(page.locator("#population")).toHaveText("6/10");
  // This tree belongs to immutable terrain, rather than the starting overlay.
  await page.evaluate(
    async (payload) => {
      const game = await import(
        new URL("/pkg/aoe_client.js", location.href).href
      );
      game.activate_surface_fixture("04".repeat(32), payload);
    },
    JSON.stringify(
      chunks([{ kind: 1, name: "tree", gather: "chop", x: 8192, y: 8187 }]),
    ),
  );
  const actors = JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as { kind: string; x: number; y: number }[];
  const worker = actors.find((actor) => actor.kind === "Villager");
  if (!worker || !socket) throw new Error("Fixture not ready");
  await page.mouse.click(worker.x, worker.y);
  await page.locator("#build-menu").click();
  await page.locator("#build-house").click();
  // A natural tree at (8192,8187), inside this otherwise clear 2x2 site.
  const id = 8187 * 2 ** 19 + 8192 * 2;
  const resource = (revision: number, remaining: number) =>
    Buffer.from([
      5,
      ...uint(subscription),
      0,
      ...uint(revision),
      1,
      ...uint(id),
      ...uint(remaining),
      0,
    ]);
  // One natural-resource override requires revision >= 1 in the current wire.
  socket.send(resource(1, 100));
  await page.mouse.move(960, 232);
  const scene = page.locator("#scene");
  await expect(scene).toHaveAttribute("data-placement-valid", "false");
  socket.send(resource(2, 1));
  await expect(scene).toHaveAttribute("data-placement-valid", "false");
  await page.mouse.click(960, 232);
  expect(orders).toHaveLength(0);
  socket.send(
    Buffer.from([5, ...uint(subscription), 1, 2, 3, 1, ...uint(id), 0, 0]),
  );
  await expect(scene).toHaveAttribute("data-placement-valid", "true");
  await page.mouse.click(960, 232);
  await expect.poll(() => orders.length).toBe(1);
  expect(orders[0]?.[0]).toBe(8); // BuildOrder, after the exhausted tile opens.
  await expect(scene).toHaveAttribute("data-order-mode", "");
});

for (const [kind, name] of [
  [5, "shore fish"],
  [6, "deep fish"],
] as const) {
  test(`Dock preview and order allow nonblocking ${name} under the water footprint`, async ({
    page,
  }) => {
    await gameAssets(page, true);
    const orders: Buffer[] = [];
    await fiveVillagerFixture(page, (bytes) => {
      if (bytes[0] === 8) orders.push(bytes);
    });
    await page.goto("/?play=1#x=8192&y=8192&zoom=1");
    await expect(page.locator("#population")).toHaveText("6/10");
    const terrain = chunks([
      { kind, name, gather: "fish", x: 8193, y: 8187 },
    ]).map((chunk) => {
      const bytes = Buffer.from(chunk.payload_hex, "hex");
      for (let index = 0; index < 1024; index++) {
        const x = chunk.x * 32 + (index % 32);
        if (x >= 8193) bytes.writeUInt32LE(11 | (3 << 12), 7 + index * 35 + 14);
      }
      return { ...chunk, payload_hex: bytes.toString("hex") };
    });
    await page.evaluate(async (payload) => {
      const game = await import(
        new URL("/pkg/aoe_client.js", location.href).href
      );
      game.activate_surface_fixture("06".repeat(32), payload);
    }, JSON.stringify(terrain));
    const worker = await page.locator("#scene").evaluate((canvas) =>
      (
        JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as {
          kind: string;
          x: number;
          y: number;
        }[]
      ).find((actor) => actor.kind === "Villager"),
    );
    if (!worker) throw new Error("Fixture villager missing");
    await page.mouse.click(worker.x, worker.y);
    await page.locator("#build-menu").click();
    await page.locator("#build-dock").click();
    await page.mouse.move(960, 232);
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-placement-valid",
      "true",
    );
    await page.mouse.click(960, 232);
    await expect.poll(() => orders.length).toBe(1);
    await expect(page.locator("#scene")).toHaveAttribute("data-order-mode", "");
  });
}
