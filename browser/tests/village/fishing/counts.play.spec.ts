import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { workerFixture } from "../fixtures/worker.js";

test("a returning Fishing Ship counts as a food worker, including after reconnect", async ({
  page,
}) => {
  await gameAssets(page);
  await workerFixture(page, true, true);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await expect(page.locator("#workers-food")).toHaveText("1");
  await expect(page.locator("#workers-gold")).toHaveText("1");
  await expect(page.locator("#workers-wood")).toHaveText("0");
  await page.reload();
  await expect(page.locator("#workers-food")).toHaveText("1");
});
