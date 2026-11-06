import { expect, type Page } from "@playwright/test";
import { isolatedTest as test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";

async function start(page: Page) {
  expect((await page.request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page, true);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
}

// The game menu's Pause stops the server clock; Resume continues it.
test("pausing from the game menu stops the match clock until resumed", async ({
  page,
}) => {
  test.setTimeout(55_000);
  await start(page);
  const clock = page.locator("#game-clock");
  await expect(clock).not.toHaveText("00:00", { timeout: 15_000 });
  await page.locator("#menu-toggle").click();
  await page.getByRole("button", { name: "Pause game" }).click();
  await expect(page.locator("#game-paused")).toBeVisible();
  await expect(page.locator("body")).toHaveAttribute("data-paused", "true");
  // A tick already in flight may land; after that the clock holds still.
  await page.waitForTimeout(1500);
  const held = await clock.textContent();
  await page.waitForTimeout(2500);
  expect(await clock.textContent()).toBe(held);

  await page.getByRole("button", { name: "Resume game" }).click();
  await expect(page.locator("#game-paused")).toBeHidden();
  await expect(clock).not.toHaveText(held ?? "", { timeout: 10_000 });
});
