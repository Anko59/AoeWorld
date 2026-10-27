import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { activateSyntheticMap } from "../synthetic-map.js";
import { freshHud } from "../village/fixtures/resources.js";
import { ownersFixture } from "./fixtures/owners.js";

const actors = async (page: import("@playwright/test").Page) =>
  JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as { id: number; x: number; y: number }[];

test("foreign selections show their owner and have no commands or orders", async ({
  page,
}) => {
  await gameAssets(page, true);
  const orders = await ownersFixture(page);
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.goto("/?play=1#x=8192.5&y=8192.5&zoom=1");
  await expect(page.locator("#connection")).toHaveText("connected");
  await activateSyntheticMap(page, () => false);
  await freshHud(page);
  for (const [id, owner] of [
    [1, "Player 2"],
    [40, "Player 4"],
  ] as const) {
    const actor = (await actors(page)).find((a) => a.id === id);
    if (!actor) throw new Error(`Missing foreign actor ${id}`);
    await page.mouse.click(actor.x, actor.y - 10);
    await expect(page.locator("#selection-owner")).toHaveText(owner);
    await expect(page.locator("#command-grid button:visible")).toHaveCount(0);
    if (id === 40) {
      // Hidden controls must also retain client-side ownership guards.
      for (const command of [
        "train-archer",
        "research-loom",
        "cancel-construction",
        "cancel-research",
        "market-buy-wood",
      ])
        await page.locator(`#${command}`).dispatchEvent("click");
    }
    await page.keyboard.press("q");
    await page.keyboard.press("s");
    await page.mouse.click(1100, 600, { button: "right" });
    await freshHud(page);
    expect(orders).toEqual([]);
  }
});
