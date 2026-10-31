import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { centerVillage } from "../../village/fixtures/resources.js";

test.use({ video: "on" });

test("connected pages both control the human seat and closing one keeps the other playable", async ({
  page,
  context,
  request,
}) => {
  test.setTimeout(90_000);
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected");
  await centerVillage(page, true);
  await expect(page.locator("#train-villager")).toBeEnabled();

  const second = await context.newPage();
  await gameAssets(second);
  // A page with no credential must still join the human seat immediately.
  await second.addInitScript(() =>
    localStorage.removeItem("aoeworld.resume-token"),
  );
  await second.goto("/?play=1");
  await expect(second.locator("#connection")).toHaveText("connected");
  await centerVillage(second, true);
  await expect(second.locator("#train-villager")).toBeEnabled();

  await page.locator("#train-villager").click();
  for (const view of [page, second]) {
    await expect(view.locator("#training-queue .unit-token")).toHaveCount(1);
  }
  await second.locator("#train-villager").click();
  for (const view of [page, second]) {
    await expect(view.locator("#training-queue .unit-token")).toHaveCount(2);
    await expect(view.locator("#connection")).toHaveText("connected");
  }
  await page.close();
  await second.locator("#train-villager").click();
  await expect(second.locator("#training-queue .unit-token")).toHaveCount(3);
  await expect(second.locator("#stockpile-food")).toHaveText("50");
  await expect(second.locator("#connection")).toHaveText("connected");
});
