import { expect, type Page } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { fiveVillagerFixture } from "../fixtures/village.js";

const stopOrder = 16;
const patrolOrder = 40;

async function selection(page: Page, kind: string, group: boolean) {
  const orders: Buffer[] = [];
  await gameAssets(page);
  await fiveVillagerFixture(
    page,
    (bytes) => {
      if ([2, stopOrder, 21, 29, 32, patrolOrder].includes(bytes[0] ?? -1))
        orders.push(bytes);
    },
    () => 0,
    false,
    true,
    kind === "General" ? 23 : 13,
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

test("the multi-selection card shows Light Cavalry x5", async ({ page }) => {
  await selection(page, "LightCavalry", true);
  await expect(page.locator("#selection-name")).toHaveText("Light Cavalry x5");
});

test("attack-move uses right clicks, ends on Shift release and cancels on Escape or Stop", async ({
  page,
}) => {
  test.setTimeout(60_000);
  const orders = await selection(page, "LightCavalry", true);
  const attack = page.locator("#attack-move-units");
  const canvas = page.locator("#scene");
  const right = { button: "right" } as const;

  await attack.click();
  await page.mouse.click(1100, 200, right);
  await expect.poll(() => orders.length).toBe(1);
  expect(orders[0]?.[0]).toBe(29);
  expect(orders[0]?.subarray(-2)).toEqual(Buffer.from([0, 1]));
  await expect(canvas).toHaveAttribute("data-order-mode", "");

  await attack.click();
  await page.keyboard.down("Shift");
  await page.mouse.click(1150, 220, right);
  await page.mouse.click(1120, 230, right);
  await expect.poll(() => orders.length).toBe(3);
  for (const order of orders.slice(1)) {
    expect(order[0]).toBe(29);
    expect(order.subarray(-2)).toEqual(Buffer.from([1, 1]));
  }
  await expect(canvas).toHaveAttribute("data-order-mode", "attack-move");
  await page.keyboard.up("Shift");
  await expect(canvas).toHaveAttribute("data-order-mode", "");
  await page.mouse.click(1100, 200, right);
  await expect.poll(() => orders.length).toBe(4);
  expect(orders[3]?.subarray(-2)).toEqual(Buffer.from([0, 0]));

  await attack.click();
  await page.keyboard.down("Shift");
  await page.keyboard.press("Escape");
  await expect(canvas).toHaveAttribute("data-order-mode", "");
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "5");
  await page.mouse.click(1150, 220, right);
  await page.keyboard.up("Shift");
  await expect.poll(() => orders.length).toBe(5);
  expect(orders[4]?.subarray(-2)).toEqual(Buffer.from([1, 0]));

  await attack.click();
  const remainingPings = await page
    .locator("#stop-units")
    .evaluate((button) => {
      (Reflect.get(window, "aoeOrderPing") as (x: number, y: number) => void)(
        500,
        300,
      );
      (button as HTMLButtonElement).click();
      return document.querySelectorAll(".order-ping").length;
    });
  expect(remainingPings).toBe(0);
  await expect(canvas).toHaveAttribute("data-order-mode", "");
  await expect.poll(() => orders.length).toBe(6);
  // This fixture's sequence and entity IDs each fit in one postcard byte.
  const [command, , count, ...stopped] = orders[5] ?? [];
  expect(command).toBe(stopOrder);
  expect(count).toBe(5);
  expect(stopped.sort((a, b) => a - b)).toEqual([2, 3, 4, 5, 6]);

  await attack.click();
  await page.keyboard.down("Shift");
  await page.mouse.click(1120, 230);
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "0");
  await expect(canvas).toHaveAttribute("data-order-mode", "");
  await page.keyboard.up("Shift");
  expect(orders).toHaveLength(6);
});

test("Patrol sends one order per selected fighter", async ({ page }) => {
  const orders = await selection(page, "LightCavalry", true);
  const canvas = page.locator("#scene");
  const patrol = page.locator("#patrol-units");
  await expect(patrol.locator('[data-ui-group="commands"]')).toHaveAttribute(
    "data-ui-id",
    "patrol",
  );
  await patrol.click();
  await expect(canvas).toHaveAttribute("data-order-mode", "patrol");
  await page.mouse.click(1100, 200, { button: "right" });
  await expect.poll(() => orders.length).toBe(5);
  expect(orders.map((order) => order[0])).toEqual(Array(5).fill(patrolOrder));
  expect(orders.map((order) => order[2]).sort()).toEqual([2, 3, 4, 5, 6]);
  await expect(canvas).toHaveAttribute("data-order-mode", "");
});

test("Stop cancels patrol placement before the next attack-move", async ({
  page,
}) => {
  const orders = await selection(page, "LightCavalry", true);
  const canvas = page.locator("#scene");
  await page.locator("#patrol-units").click();
  await expect(canvas).toHaveAttribute("data-order-mode", "patrol");
  await page.locator("#stop-units").click();
  await expect(canvas).toHaveAttribute("data-order-mode", "");
  await page.mouse.click(1100, 200, { button: "right" });
  await expect
    .poll(() => orders.map((order) => order[0]))
    .toEqual([stopOrder, 29]);
  expect(orders[1]?.subarray(-2)).toEqual(Buffer.from([0, 0]));

  await page.locator("#attack-move-units").click();
  await page.mouse.click(1100, 200, { button: "right" });
  await expect.poll(() => orders.length).toBe(3);
  expect(orders[2]?.subarray(-2)).toEqual(Buffer.from([0, 1]));
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
