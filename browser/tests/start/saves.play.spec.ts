import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { completedJob, setup } from "./skirmish.js";

// Save a running game from the game menu, quit, and resume it from the main
// menu's Load Game list. The map, its activation and the saved match are real.
test.beforeEach(async ({ page, request }) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
});

test("a saved game is listed on the main menu and resumes where it was", async ({
  page,
}) => {
  test.setTimeout(180_000);
  const hash = process.env["AOE_E2E_LANDSCAPE_HASH"];
  if (!hash) throw new Error("The E2E stack must supply a prepared map");
  await completedJob(page, hash);
  await setup(page);
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

  await page.getByRole("button", { name: "Load Game" }).click();
  await expect(page.getByRole("heading", { name: "Load Game" })).toBeVisible();
  const box = await page.locator(".saves-panel").boundingBox();
  const size = page.viewportSize();
  if (!box || !size) throw new Error("saves panel is missing");
  expect(box.y).toBeGreaterThanOrEqual(0);
  expect(box.y + box.height).toBeLessThanOrEqual(size.height);
  expect(box.x + box.width).toBeLessThanOrEqual(size.width);
  const row = page.locator("#saves-list li", { hasText: "E2E save" });
  await expect(row).toBeVisible();
  await row.getByRole("button", { name: "Resume E2E save" }).click();
  await expect(page.locator("#start-sequence")).toBeHidden({
    timeout: 120_000,
  });
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect(page.locator("#minimap")).toHaveAttribute("data-map-hash", hash);
  await expect(page.locator("#population")).toHaveText(population ?? "");
  // The clock continues from the saved time, not from zero.
  const resumed = (await page.locator("#game-clock").textContent()) ?? "";
  expect(resumed).not.toBe("00:00");
  expect(resumed >= (saved ?? "")).toBe(true);

  // Clean up: delete the save from the list.
  await page.getByRole("button", { name: "Menu", exact: true }).click();
  await page.getByRole("button", { name: "Quit to Main Menu" }).click();
  await page.getByRole("button", { name: "Load Game" }).click();
  await page.getByRole("button", { name: "Delete E2E save" }).click();
  await expect(row).toHaveCount(0);
});
