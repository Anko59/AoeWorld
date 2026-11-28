import { expect } from "@playwright/test";
import { isolatedTest as test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";

test("the food HUD shows the upkeep bill, deduction and shortage in a live match", async ({
  page,
  request,
}) => {
  test.setTimeout(65_000);
  await gameAssets(page, true);
  expect(
    (await request.post("/maps/reset?scenario=food-upkeep")).status(),
  ).toBe(204);
  await page.goto("/?play=1#x=128&y=134&zoom=1");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });

  const upkeep = page.locator("#food-upkeep");
  const food = page.locator("#stockpile-food");
  const shortage = page.locator("#food-upkeep-shortage");
  await expect(upkeep).toHaveText("−31/min");
  await expect(shortage).toBeHidden();
  await expect(food).toHaveText("12");
  await page.getByRole("button", { name: "Resume game" }).click();
  await expect(food).toHaveText(/^(?:[1-9]|1[01])$/, { timeout: 8_000 });
  await expect(food).toHaveText("0", { timeout: 26_000 });
  await expect(shortage).toBeVisible({ timeout: 4_000 });
  await expect(shortage).toHaveText("Army food shortage");
});
