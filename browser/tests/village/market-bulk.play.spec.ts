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
  // The preview is computed when the client draws a frame, a while after the
  // pointer moved (two animation frames were too few: 16 of 30 repeats found no
  // site). So each probe waits, up to a bound, for the attribute to turn true,
  // and a click that places nothing (the attribute was a frame behind and the
  // site is not valid) goes on to the next probe instead of ending the search.
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-order-mode",
    "build",
  );
  const marketPlaced = () =>
    expect
      .poll(async () => (await actors(page)).some((a) => a.kind === "Market"), {
        timeout: 1_500,
        intervals: [100],
      })
      .toBe(true)
      .then(
        () => true,
        () => false,
      );
  let placedAt: [number, number] | undefined;
  for (const y of [180, 260, 340, 420])
    for (const x of [150, 280, 410, 540, 670, 800, 930, 1060]) {
      if (placedAt) break;
      await page.mouse.move(x, y);
      const valid = await expect
        .poll(
          () => page.locator("#scene").getAttribute("data-placement-valid"),
          { timeout: 400, intervals: [50] },
        )
        .toBe("true")
        .then(
          () => true,
          () => false,
        );
      if (!valid) continue;
      await page.mouse.click(x, y);
      if (await marketPlaced()) placedAt = [x, y];
    }
  expect(placedAt).toBeDefined();
  await expect
    .poll(async () => (await actors(page)).some((a) => a.kind === "Market"), {
      timeout: 30_000,
    })
    .toBe(true);
  // A Market is large and the first valid site can lie low on the screen: its
  // centre then sits under the bottom panel (CI: centre y 520, panel from y 512)
  // and a click there selects nothing. Click inside the footprint (about 30
  // above and below its centre), no lower than the panel's top edge.
  const clickable = (market: Actor): [number, number][] =>
    [...new Set([0, -10, -20, -28].map((dy) => Math.min(market.y + dy, 500)))]
      .filter((y) => y > 54)
      .map((y): [number, number] => [market.x, y]);
  await expect
    .poll(
      async () => {
        const market = (await actors(page)).find((a) => a.kind === "Market");
        for (const [x, y] of market ? clickable(market) : []) {
          await page.mouse.click(x, y);
          const name = await page.locator("#selection-name").textContent();
          if (name === "Market") return name;
        }
        return page.locator("#selection-name").textContent();
      },
      { timeout: 30_000 },
    )
    .toBe("Market");
  await test.info().attach("placement.json", {
    body: JSON.stringify({
      placedAt,
      market: (await actors(page)).find((a) => a.kind === "Market"),
    }),
    contentType: "application/json",
  });
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
