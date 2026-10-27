import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { syntheticChunk } from "../../synthetic-map.js";
import { inputFixture, screen, selectWorker } from "./fixture.js";

for (const ford of [false, true]) {
  test(`ship can target ${ford ? "wadeable marsh" : "shallow coastal water"}`, async ({
    page,
  }) => {
    const commands = await inputFixture(
      page,
      "webgl2",
      [],
      false,
      0,
      undefined,
      { cargo: 0 },
    );
    const chunks = [-1, 0].flatMap((y) =>
      [-1, 0].map((x) => {
        const chunk = syntheticChunk(256 + x, 256 + y, () => false);
        const bytes = Buffer.from(chunk.payload_hex, "hex");
        for (let i = 0; i < 1024; i++) {
          const tx = chunk.x * 32 + (i % 32);
          const ty = chunk.y * 32 + Math.floor(i / 32);
          const wet = tx >= 8193 && ty <= 8193;
          bytes.writeUInt32LE(
            wet
              ? ford
                ? 13 | (3 << 12) | (1 << 21)
                : 10 | (1 << 12)
              : 1 << 21,
            7 + i * 35 + 14,
          );
        }
        let payload = bytes;
        if (!ford && chunk.x === 256 && chunk.y === 256) {
          // Fish are navigable resources, including on shallow coastal water.
          const fish = Buffer.alloc(22);
          fish.writeBigUInt64LE(BigInt(8192) * 524288n + BigInt(8195) * 2n, 0);
          fish.writeInt32LE(8195, 8);
          fish.writeInt32LE(8192, 12);
          fish[16] = 0; // Food.
          fish[17] = 5; // ShoreFish.
          fish.writeUInt16LE(200, 18);
          fish[21] = 5; // ShoreFish visual family.
          bytes.writeUInt16LE(1, 3);
          payload = Buffer.concat([bytes, fish]);
        }
        return { ...chunk, payload_hex: payload.toString("hex") };
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
    await page.keyboard.press("Home");
    const ship = screen(8193.5, 8194.5);
    await page.mouse.click(ship.x, ship.y);
    await expect(page.locator("#selection-name")).toHaveText("Transport Ship");
    const target = screen(8195.5, 8192.5);
    await page.mouse.move(target.x, target.y);
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-command-cursor",
      "move",
    );
    await page.mouse.click(target.x, target.y, { button: "right" });
    await expect
      .poll(() => commands.some((b) => b[0] === 2 && b[2] === 5))
      .toBe(true);
    if (ford) {
      await selectWorker(page);
      await page.mouse.move(target.x, target.y);
      await expect(page.locator("#scene")).toHaveAttribute(
        "data-command-cursor",
        "move",
      );
      await page.mouse.click(target.x, target.y, { button: "right" });
      await expect
        .poll(() => commands.some((b) => b[0] === 2 && b[2] === 2))
        .toBe(true);
    }
  });
}
