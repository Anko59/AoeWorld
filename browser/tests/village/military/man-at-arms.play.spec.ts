import { expect, type Page } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { centerVillage, freshHud } from "../fixtures/resources.js";

type Actor = { id: number; kind: string; x: number; y: number };
async function actors(page: Page): Promise<Actor[]> {
  return JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as Actor[];
}
async function select(page: Page, kind: string) {
  const actor = (await actors(page)).find(
    (a) => a.kind === kind && a.x > 0 && a.x < 1280 && a.y > 54 && a.y < 512,
  );
  if (!actor) throw new Error(`Visible ${kind} missing`);
  await page.mouse.click(actor.x, actor.y);
  await expect(page.locator("#selection")).toHaveAttribute("data-kind", kind);
}

test("Barracks explains the upgrade gate and researches existing and future Men-at-Arms", async ({
  page,
  request,
}) => {
  test.setTimeout(90_000);
  expect(
    (await request.post("/maps/reset?scenario=feudal-economy")).status(),
  ).toBe(204);
  await gameAssets(page);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
  await centerVillage(page);
  // Keep the northern Barracks below the top bar and inside the viewport.
  await page.mouse.move(640, 360);
  await page.mouse.wheel(0, 288);
  await freshHud(page);
  await select(page, "Villager");
  await page.keyboard.press("b");
  await page.keyboard.press("q");
  let placed = false;
  for (const [x, y] of [
    [920, 360],
    [900, 260],
    [400, 240],
    [420, 420],
  ] as const) {
    await page.mouse.move(x, y);
    await freshHud(page);
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
    .poll(async () => (await actors(page)).some((a) => a.kind === "House"))
    .toBe(true);
  await select(page, "House");
  await expect(page.locator("#building-status")).toContainText("Complete");
  await select(page, "Barracks");
  const upgrade = page.locator("#research-man-at-arms");
  const train = page.locator("#train-villager");
  await expect(upgrade).toContainText("Research Man-at-Arms");
  await expect(upgrade).toBeDisabled();
  await expect(upgrade).toHaveAttribute("title", /requires Feudal Age/);
  await expect(train).toHaveAttribute("title", /Train Militia/);
  await expect(train).toContainText("Militia");
  await train.click();
  await expect
    .poll(
      async () =>
        (await actors(page)).filter((a) => a.kind === "Militia").length,
    )
    .toBe(1);
  await centerVillage(page, true);
  await page.locator("#research-feudal-age").click();
  await expect(page.locator("#age")).toContainText("Feudal Age", {
    timeout: 20_000,
  });
  await select(page, "Barracks");
  await expect(upgrade).toBeEnabled();
  await train.click();
  await train.click();
  await train.click();
  await expect(upgrade).toBeDisabled();
  await expect(upgrade).toHaveAttribute("title", /training queue/);
  await expect(upgrade).toBeEnabled();
  await upgrade.click();
  await expect(train).toBeDisabled();
  await expect(train).toHaveAttribute("title", /Research in progress/);
  await expect
    .poll(
      async () =>
        (await actors(page)).filter((a) => a.kind === "ManAtArms").length,
    )
    .toBe(4);
  await expect(upgrade).toBeDisabled();
  await expect(upgrade).toHaveAttribute("title", /Already researched/);
  await expect(train).toBeEnabled();
  await expect(train).toHaveAttribute("title", /Train Man-at-Arms/);
  await expect(train).toContainText("Man-at-Arms");
  await train.click();
  await expect
    .poll(
      async () =>
        (await actors(page)).filter((a) => a.kind === "ManAtArms").length,
    )
    .toBe(5);
  expect((await actors(page)).some((a) => a.kind === "Militia")).toBe(false);
});
