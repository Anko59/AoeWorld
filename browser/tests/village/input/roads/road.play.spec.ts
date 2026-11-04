import { expect } from "@playwright/test";
import { test } from "../../../../server.js";
import { gameAssets } from "../../../game-assets.js";
import { fiveVillagerFixture } from "../../fixtures/village.js";
import { centerVillage } from "../../fixtures/resources.js";
import { sint } from "../fixture.js";

test("Dark Age Road previews a line and sends the appended kind with Shift", async ({
  page,
}) => {
  await gameAssets(page, true);
  const orders: Buffer[] = [];
  await fiveVillagerFixture(page, (bytes) => {
    if (bytes[0] === 9) orders.push(bytes);
  });
  await page.goto("/?play=1#x=8192&y=8192&zoom=1");
  await expect(page.locator("#population")).toHaveText("6/10");
  await centerVillage(page);
  const actors = JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as { kind: string; x: number; y: number }[];
  const worker = actors.find((a) => a.kind === "Villager");
  if (!worker) throw Error("Villager missing");
  await page.mouse.click(worker.x, worker.y);
  await page.locator("#build-military-menu").click();
  await expect(page.locator("#build-road")).toBeEnabled();
  await page.locator("#build-road").click();
  const size = page.viewportSize();
  if (!size) throw Error("Viewport missing");
  const point = (x: number, y: number) => ({
    x: size.width / 2 + (x - y) * 64,
    y: size.height / 2 + (x + y - 16384) * 32,
  });
  const from = point(8188.5, 8189.5),
    to = point(8192.5, 8189.5);
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(to.x, to.y, { steps: 5 });
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-placement-valid",
    "true",
  );
  await page.keyboard.down("Shift");
  await page.mouse.up();
  await page.keyboard.up("Shift");
  await expect.poll(() => orders.length).toBe(1);
  const order = orders[0];
  if (!order) throw Error("Road order missing");
  expect([...order.subarray(4)]).toEqual([
    23,
    ...sint(8188),
    ...sint(8189),
    ...sint(8192),
    ...sint(8189),
    1,
  ]);
  await page.keyboard.press("Escape");
});
