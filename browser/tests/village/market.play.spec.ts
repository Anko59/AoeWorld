import { expect, type Page } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { centerVillage } from "./fixtures/resources.js";
import { forceCanvas } from "../rendering/backends.js";

type Actor = { kind: string; x: number; y: number };
async function actors(page: Page): Promise<Actor[]> {
  return JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as Actor[];
}
async function select(page: Page, kind: string) {
  const actor = (await actors(page)).find((a) => a.kind === kind);
  if (!actor) throw new Error(`${kind} missing`);
  await page.mouse.click(actor.x, actor.y);
}
const count = async (page: Page, id: string) =>
  Number(await page.locator(`#stockpile-${id}`).textContent());

// The explicit feudal-economy scenario starts rich with a Mill; the same
// documented fast path as feudal.play.spec.ts reaches Feudal in seconds.
test("reach Feudal, build a Market, sell food and buy wood for gold", async ({
  page,
  request,
}, info) => {
  test.setTimeout(120_000);
  if (info.project.name === "canvas") await forceCanvas(page);
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
    timeout: 20_000,
  });
  await select(page, "Villager");
  await page.locator("#build-menu").click();
  await expect(page.locator("#build-market")).toBeEnabled();
  await page.keyboard.press("d"); // DE economic grid: B, D = Market
  let placed = false;
  // A 4x4 footprint needs open ground: sweep the visible map for a valid one.
  const spots: [number, number][] = [];
  for (const y of [180, 260, 340, 420])
    for (const x of [150, 280, 410, 540, 670, 800, 930, 1060])
      spots.push([x, y]);
  for (const [x, y] of spots) {
    await page.mouse.move(x, y);
    await expect
      .poll(() => page.locator("#scene").getAttribute("data-placement-valid"))
      .toMatch(/true|false/);
    await page.waitForTimeout(150);
    if (
      (await page.locator("#scene").getAttribute("data-placement-valid")) ===
      "true"
    ) {
      await page.mouse.click(x, y);
      placed = true;
      break;
    }
  }
  expect(placed).toBe(true);
  await expect
    .poll(async () => (await actors(page)).some((a) => a.kind === "Market"))
    .toBe(true);
  // Builders stand on the foundation, so retry the pick until it is the Market.
  await expect
    .poll(
      async () => {
        await select(page, "Market");
        return page.locator("#selection-name").textContent();
      },
      { timeout: 30_000 },
    )
    .toBe("Market");
  await expect(page.locator("#building-status")).toContainText("Complete", {
    timeout: 30_000,
  });
  await expect(page.locator("#market-prices")).toContainText(
    "wood 100 · food 100 · stone 100",
  );
  await expect(page.locator("#market-sell-food")).toBeEnabled();
  await expect(page.locator("#market-sell-food")).toContainText("70 gold");

  const [food, gold] = [await count(page, "food"), await count(page, "gold")];
  await page.locator("#market-sell-food").click();
  await expect(page.locator("#stockpile-food")).toHaveText(String(food - 100));
  await expect(page.locator("#stockpile-gold")).toHaveText(String(gold + 70));
  await expect(page.locator("#market-prices")).toContainText("food 97");
  await expect(page.locator("#market-sell-food")).toContainText("67 gold");

  const wood = await count(page, "wood");
  await page.locator("#market-buy-wood").click();
  await expect(page.locator("#stockpile-wood")).toHaveText(String(wood + 100));
  await expect(page.locator("#stockpile-gold")).toHaveText(
    String(gold + 70 - 130),
  );
  await expect(page.locator("#market-prices")).toContainText("wood 103");

  // Only a completed Market shows the panel.
  await select(page, "TownCenter");
  await expect(page.locator("#market-sell-food")).toBeHidden();
});
