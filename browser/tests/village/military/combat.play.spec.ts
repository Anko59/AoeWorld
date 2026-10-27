import { expect, type Page } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { centerVillage, freshHud } from "../fixtures/resources.js";

type Actor = {
  id: number;
  kind: string;
  owned: boolean;
  x: number;
  y: number;
  hp: number;
  maxHp: number;
};
const actors = async (page: Page): Promise<Actor[]> =>
  JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  );

test("a Castle rally survives reload without poisoning the game snapshot", async ({
  page,
  request,
}, info) => {
  test.setTimeout(90_000);
  expect(
    (await request.post("/maps/reset?scenario=vassal-economy")).status(),
  ).toBe(204);
  await gameAssets(page, true);
  await page.goto("/?play=1#x=137&y=132&zoom=.5");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await freshHud(page);
  const scene = page.locator("#scene");
  const castle = (await actors(page)).find((a) => a.kind === "Castle");
  if (!castle) throw new Error("Castle missing from the actual server world");
  await page.mouse.click(castle.x, castle.y);
  await expect(page.locator("#selection-name")).toHaveText("Castle");
  await page.mouse.click(castle.x + 110, castle.y, { button: "right" });
  await expect(scene).toHaveAttribute("data-rally-point", /.+/);
  await expect(page.locator("#connection")).toHaveText("connected");
  await page.reload();
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
  await expect(page.locator("#game-error")).toBeHidden();
  await page.screenshot({ path: info.outputPath("castle-rally.png") });
});

test("military stance buttons update the authoritative server state", async ({
  page,
  request,
}, info) => {
  test.setTimeout(90_000);
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page, true);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await centerVillage(page);
  await freshHud(page);
  const scene = page.locator("#scene");
  const scout = (await actors(page)).find((a) => a.kind === "Scout" && a.owned);
  if (!scout) throw new Error("Owned Scout missing");
  await page.mouse.click(scout.x, scout.y);
  await expect(page.locator("#selection")).toHaveAttribute(
    "data-kind",
    "Scout",
  );
  await expect(page.getByLabel("Combat stance")).toBeInViewport({ ratio: 1 });
  await page.getByLabel("Combat stance").selectOption("3");
  await expect(scene).toHaveAttribute("data-selected-stance", "3");
  await expect(page.getByLabel("Combat stance")).toHaveValue("3");
  await page.getByLabel("Combat stance").selectOption("0");
  await expect(scene).toHaveAttribute("data-selected-stance", "0");
  await expect(page.locator("#game-error")).toBeHidden();
  await page.screenshot({
    path: info.outputPath("combat-stance.png"),
  });
});
