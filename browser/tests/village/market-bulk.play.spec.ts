import { expect, type Page } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { centerVillage } from "./fixtures/resources.js";
import { forceWebGl } from "../rendering/backends.js";
import { openTab } from "../rendering/fixtures/cockpit-tabs.js";

type Actor = { kind: string; x: number; y: number };
const actors = async (page: Page): Promise<Actor[]> =>
  JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  );
const count = async (page: Page, id: string) =>
  Number(await page.locator(`#stockpile-${id}`).textContent());

// The same lot-by-lot rule as the server: each 100-unit lot pays 70% of the
// price the previous lot left, the price falls 3 per lot and stops at 20.
function proceeds(lots: number, price = 100) {
  let gold = 0;
  for (let i = 0; i < lots; i++) {
    gold += Math.floor((price * 7000) / 10000);
    price = Math.max(20, price - 3);
  }
  return gold;
}

// A real client and server: feudal-economy starts with 120,000 food. Build a
// Market through the ordinary UI, then sell all of it from the far Trade tab.
test("sell 100,000 food from the far Trade tab and watch the price recover", async ({
  page,
  request,
}) => {
  test.setTimeout(240_000);
  await forceWebGl(page);
  expect(
    (await request.post("/maps/reset?scenario=feudal-economy")).status(),
  ).toBe(204);
  await gameAssets(page);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
  await centerVillage(page, true);
  await page.locator("#research-feudal-age").click();
  await expect(page.locator("#age")).toContainText("Feudal Age", {
    timeout: 30_000,
  });
  const villager = (await actors(page)).find((a) => a.kind === "Villager");
  if (!villager) throw new Error("Villager missing");
  await page.mouse.click(villager.x, villager.y);
  await page.locator("#build-menu").click();
  await expect(page.locator("#build-market")).toBeEnabled();
  await page.keyboard.press("d");
  let placed = false;
  for (const y of [180, 260, 340, 420])
    for (const x of [150, 280, 410, 540, 670, 800, 930, 1060]) {
      if (placed) break;
      await page.mouse.move(x, y);
      await page.waitForTimeout(150);
      if (
        (await page.locator("#scene").getAttribute("data-placement-valid")) ===
        "true"
      ) {
        await page.mouse.click(x, y);
        placed = true;
      }
    }
  expect(placed).toBe(true);
  await expect
    .poll(async () => (await actors(page)).some((a) => a.kind === "Market"), {
      timeout: 30_000,
    })
    .toBe(true);
  await expect
    .poll(
      async () => {
        const market = (await actors(page)).find((a) => a.kind === "Market");
        if (market) await page.mouse.click(market.x, market.y);
        return page.locator("#selection-name").textContent();
      },
      { timeout: 30_000 },
    )
    .toBe("Market");
  await expect(page.locator("#building-status")).toContainText("Complete", {
    timeout: 60_000,
  });

  const [food, gold] = [await count(page, "food"), await count(page, "gold")];
  expect(food).toBeGreaterThanOrEqual(100_000);
  // Pull back to the far view and trade there.
  await page.evaluate(() => {
    location.hash = "zoom=.04";
  });
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-lod",
    /markers|map/,
    {
      timeout: 30_000,
    },
  );
  await openTab(page, "trade");
  await page.locator('[data-res="food"].far-cell').click();
  await page.locator('[data-side="sell"]').click();
  await page.locator('input[aria-label="Units to trade"]').fill("100000");
  const quote = page.locator("[data-quote]");
  // The quote is honest about the floor: most lots pay 14, not 70.
  await expect(quote).toContainText("1,000 lots");
  await expect(quote).toContainText("limit (20)");
  await expect(quote).toContainText("pay 14 gold each instead of 70");
  const expected = proceeds(1000);
  await page.locator("[data-go]").click();
  await expect(page.locator("#stockpile-food")).toHaveText(
    String(food - 100_000),
    {
      timeout: 20_000,
    },
  );
  await expect(page.locator("#stockpile-gold")).toHaveText(
    String(gold + expected),
  );
  await expect(
    page.locator('[data-res="food"].far-cell .far-cell-note'),
  ).toHaveText("20 g / 100");
  // The market keeps working: the price climbs back a gold at a time.
  await expect
    .poll(
      async () =>
        Number(
          (
            (await page
              .locator('[data-res="food"].far-cell .far-cell-note')
              .textContent()) ?? ""
          ).split(" ")[0],
        ),
      { timeout: 30_000 },
    )
    .toBeGreaterThan(20);
});
