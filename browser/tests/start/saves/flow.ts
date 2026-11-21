import { expect, type Page } from "@playwright/test";

export async function saveMatch(page: Page) {
  await page.getByRole("button", { name: "Start Game", exact: true }).click();
  await expect(page.locator("#start-sequence")).toBeHidden({ timeout: 90_000 });
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect(page.locator("#game-clock")).not.toHaveText("00:00", {
    timeout: 20_000,
  });
  const population = await page.locator("#population").textContent();

  await page.getByRole("button", { name: "Menu", exact: true }).click();
  await page.getByLabel("Save name").fill("E2E save");
  await page.getByRole("button", { name: "Save game", exact: true }).click();
  await expect(page.locator("#save-status")).toContainText("Saved “E2E save”", {
    timeout: 30_000,
  });
  const saved = await page.locator("#game-clock").textContent();
  await page.getByRole("button", { name: "Quit to Main Menu" }).click();
  await expect(page.locator("#main-menu")).toBeVisible();
  return { population, saved };
}
