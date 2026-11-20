import { expect } from "@playwright/test";
import { isolatedTest as test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { fiveVillagerFixture } from "../fixtures/village.js";

test("a thousand same-kind soldiers issue one frame for each army order", async ({
  page,
}) => {
  test.setTimeout(60_000);
  const orders: Buffer[] = [];
  await gameAssets(page);
  await fiveVillagerFixture(
    page,
    (bytes) => {
      if ([2, 7, 29, 41, 42].includes(bytes[0] ?? -1)) orders.push(bytes);
    },
    () => 0,
    false,
    true,
    13,
    1000,
  );
  await page.goto("/?play=1#x=8192&y=8195&zoom=.7");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
  const canvas = page.locator("#scene");
  await expect(canvas).toHaveAttribute("data-actors", /"id":1002/);
  const actors = await canvas.evaluate(
    (node) =>
      JSON.parse(node.getAttribute("data-actors") ?? "[]") as {
        id: number;
        x: number;
        y: number;
      }[],
  );
  const army = actors.filter((actor) => actor.id >= 2 && actor.id < 1002);
  const enemy = actors.find((actor) => actor.id === 1002);
  expect(army).toHaveLength(1000);
  if (!enemy) throw new Error("The enemy must be in view");
  const xs = army.map((actor) => actor.x);
  const ys = army.map((actor) => actor.y);
  await page.mouse.move(Math.min(...xs) - 5, Math.min(...ys) - 5);
  await page.mouse.down();
  await page.mouse.move(Math.max(...xs) + 5, Math.max(...ys) + 5, { steps: 5 });
  await page.mouse.up();
  await expect(page.locator("#selection")).toHaveAttribute(
    "data-count",
    "1000",
  );
  await page.mouse.click(1100, 200, { button: "right" });
  await expect.poll(() => orders.length).toBe(1);
  await page.locator("#attack-move-units").click();
  await page.mouse.click(1100, 200, { button: "right" });
  await expect.poll(() => orders.length).toBe(2);
  await page.mouse.click(enemy.x, enemy.y, { button: "right" });
  await expect.poll(() => orders.length).toBe(3);
  expect(orders.map((order) => order[0])).toEqual([29, 29, 42]);
  const packedIds = 126 + 2 * (1000 - 126);
  expect(orders[2]).toHaveLength(2 + 2 + packedIds + 2);
});
