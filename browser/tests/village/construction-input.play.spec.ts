import { expect, type WebSocketRoute } from "@playwright/test";
import { isolatedTest as test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { fiveVillagerFixture } from "./fixtures/village.js";
import { freshHud } from "./fixtures/resources.js";
import { fogState } from "../wire/fog.js";

test("placement requires explored footprint tiles even on resident terrain", async ({
  page,
}) => {
  await gameAssets(page, true);
  let socket: WebSocketRoute | undefined;
  const orders: number[] = [];
  await fiveVillagerFixture(page, (bytes, ws) => {
    socket = ws;
    if (bytes[0] !== undefined && bytes[0] > 1) orders.push(bytes[0]);
  });
  await page.goto("/?play=1#x=8192&y=8192&zoom=1");
  await expect(page.locator("#population")).toHaveText("6/10");
  const actors = JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as { kind: string; x: number; y: number }[];
  const worker = actors.find((actor) => actor.kind === "Villager");
  if (!worker) throw new Error("Fixture villager missing");
  await page.mouse.click(worker.x, worker.y);
  await page.locator("#build-menu").click();
  await page.locator("#build-house").click();
  await page.mouse.move(1000, 240);
  const scene = page.locator("#scene");
  await expect(scene).toHaveAttribute("data-placement-valid", "true");
  if (!socket) throw new Error("Fixture socket missing");
  const uint = (n: number) => {
    const bytes: number[] = [];
    do {
      const low = n % 128;
      n = Math.floor(n / 128);
      bytes.push(low + (n ? 128 : 0));
    } while (n);
    return bytes;
  };
  socket.send(
    fogState(uint, 1, 1, [8170, 8170, 8220, 8220], new Array(2500).fill(0)),
  );
  await expect(scene).toHaveAttribute("data-placement-valid", "false");
  await page.mouse.click(1000, 240);
  await freshHud(page);
  expect(orders).toEqual([]);
  await expect(scene).toHaveAttribute("data-order-mode", "build");
  // Explored grey fog is legal: current visibility is not required.
  socket.send(
    fogState(uint, 1, 2, [8170, 8170, 8220, 8220], new Array(2500).fill(1)),
  );
  await expect(scene).toHaveAttribute("data-placement-valid", "true");
  await page.mouse.click(1000, 240);
  await expect.poll(() => orders.length).toBe(1);
});

for (const button of ["middle", "right"] as const) {
  test(`${button} release ends a placement pan without a build order`, async ({
    page,
  }) => {
    await gameAssets(page, true);
    const orders: number[] = [];
    await fiveVillagerFixture(page, (bytes) => {
      if (bytes[0] !== undefined && bytes[0] > 1) orders.push(bytes[0]);
    });
    await page.goto("/?play=1#x=8192&y=8192&zoom=1");
    await expect(page.locator("#population")).toHaveText("6/10");
    const actors = JSON.parse(
      (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
    ) as { kind: string; x: number; y: number }[];
    const worker = actors.find((actor) => actor.kind === "Villager");
    if (!worker) throw new Error("Fixture villager missing");
    await page.mouse.click(worker.x, worker.y);
    await page.locator("#build-menu").click();
    await page.locator("#build-house").click();
    await page.mouse.move(1000, 240);
    await page.mouse.down({ button: "middle" });
    await page.mouse.move(1040, 260, { steps: 4 });
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-placement-valid",
      "true",
    );
    if (button === "middle") await page.mouse.up({ button });
    else {
      // A secondary release while the middle pan owns capture must also clear it.
      await page.locator("#scene").dispatchEvent("pointerup", {
        button: 2,
        pointerId: 1,
        clientX: 1040,
        clientY: 260,
      });
    }
    await freshHud(page);
    expect(orders).toEqual([]);
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-order-mode",
      "build",
    );
    const camera = () =>
      page
        .locator("#minimap")
        .evaluate((node) => [
          node.getAttribute("data-camera-x"),
          node.getAttribute("data-camera-y"),
        ]);
    const settled = await camera();
    await page.mouse.move(1080, 280);
    await freshHud(page);
    expect(await camera()).toEqual(settled);
    if (button === "right") await page.mouse.up({ button: "middle" });
  });
}
