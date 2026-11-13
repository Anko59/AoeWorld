import { expect, type Page } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { fiveVillagerFixture } from "../fixtures/village.js";

async function selection(page: Page, kind: string, group: boolean) {
  const orders: Buffer[] = [];
  await gameAssets(page);
  await fiveVillagerFixture(
    page,
    (bytes) => {
      if ([2, 21, 29, 32].includes(bytes[0] ?? -1)) orders.push(bytes);
    },
    () => 0,
    false,
    true,
    kind === "General" ? 23 : 2,
  );
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30000,
  });
  const canvas = page.locator("#scene");
  await expect.poll(() => canvas.getAttribute("data-actors")).toContain(kind);
  const actor = await canvas.evaluate(
    (canvas, kind) =>
      (
        JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as {
          kind: string;
          x: number;
          y: number;
        }[]
      ).find((actor) => actor.kind === kind),
    kind,
  );
  if (!actor) throw new Error(`${kind} fixture missing`);
  if (group) await page.mouse.dblclick(actor.x, actor.y);
  else await page.mouse.click(actor.x, actor.y);
  await expect(page.locator("#selection")).toHaveAttribute(
    "data-count",
    group ? "5" : "1",
  );
  return orders;
}

test("Shift-left-click appends attack-move destinations while the cursor stays armed", async ({
  page,
}) => {
  const orders = await selection(page, "Militia", true);
  await page.locator("#attack-move-units").click();
  await page.mouse.click(1100, 200);
  await expect.poll(() => orders.length).toBe(1);
  expect(orders[0]?.[0]).toBe(29);
  expect(orders[0]?.at(-2)).toBe(0);
  expect(orders[0]?.at(-1)).toBe(1);
  await page.locator("#attack-move-units").click();
  await page.keyboard.down("Shift");
  await page.mouse.click(1150, 220);
  await expect.poll(() => orders.length).toBe(2);
  expect(orders[1]?.[0]).toBe(29);
  expect(orders[1]?.at(-2)).toBe(1);
  expect(orders[1]?.at(-1)).toBe(1);
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-order-mode",
    "attack-move",
  );
  await page.mouse.click(1120, 230);
  await page.keyboard.up("Shift");
  await expect.poll(() => orders.length).toBe(3);
  expect(orders[2]?.at(-2)).toBe(1);
  expect(orders[2]?.at(-1)).toBe(1);
});

test("Shift-right-click queues the selected General's next army destination", async ({
  page,
}) => {
  const orders = await selection(page, "General", false);
  await page.mouse.click(1100, 200, { button: "right" });
  await expect.poll(() => orders.length).toBe(1);
  expect(orders[0]?.[0]).toBe(2);
  await page.keyboard.down("Shift");
  await page.mouse.click(1150, 220, { button: "right" });
  await page.keyboard.up("Shift");
  await expect.poll(() => orders.length).toBe(2);
  expect(orders[1]?.[0]).toBe(21);
  expect(orders[1]?.[2]).toBe(orders[0]?.[2]);
});
