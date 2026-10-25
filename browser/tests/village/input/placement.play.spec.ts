import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { fiveVillagerFixture } from "../fixtures/village.js";
import { freshHud } from "../fixtures/resources.js";
import { arm } from "../fixtures/placement.js";

for (const end of ["Shift", "Escape", "right", "blur"] as const) {
  test(`Shift places distinct queued foundations once per click and ${end} ends placement`, async ({
    page,
  }) => {
    await gameAssets(page, true, [[2223, 3]]);
    const orders: number[][] = [];
    await fiveVillagerFixture(page, (bytes) => {
      if (bytes[0] === 8) orders.push([...bytes]);
    });
    await arm(page);
    await page.keyboard.down("Shift");
    await page.mouse.click(1000, 240);
    await expect.poll(() => orders.length).toBe(1);
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-order-mode",
      "build",
    );
    await page.mouse.click(1070, 290);
    await expect.poll(() => orders.length).toBe(2);
    expect(orders.every((order) => order.at(-1) === 1)).toBe(true);
    expect(orders[0]?.slice(3, -1)).not.toEqual(orders[1]?.slice(3, -1));
    if (end === "right") await page.mouse.click(1070, 290, { button: "right" });
    else if (end === "Escape") await page.keyboard.press("Escape");
    else if (end === "blur")
      await page.evaluate(() => window.dispatchEvent(new Event("blur")));
    else await page.keyboard.up("Shift");
    await expect(page.locator("#scene")).toHaveAttribute("data-order-mode", "");
    await freshHud(page);
    expect(orders).toHaveLength(2);
    if (end !== "Shift") await page.keyboard.up("Shift");
  });
}
