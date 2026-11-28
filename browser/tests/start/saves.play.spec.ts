import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { saveMatch } from "./saves/flow.js";
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
  const { population, saved } = await saveMatch(page);
  const saveId = await page.locator("#save-status").getAttribute("data-saved");
  if (!saveId)
    throw new Error("The game menu did not report the saved game ID");

  // Reopen the start page without a controller token. Hold the game client
  // download so opening Load Game always happens before Welcome.
  await page.addInitScript(() =>
    localStorage.removeItem("aoeworld.resume-token"),
  );
  let releaseClient!: () => void;
  const clientReady = new Promise<void>((resolve) => {
    releaseClient = resolve;
  });
  await page.route("**/pkg/aoe_client_bg.wasm", async (route) => {
    await clientReady;
    await route.continue();
  });
  await page.reload({ waitUntil: "domcontentloaded" });

  await page.getByRole("button", { name: "Load Game" }).click();
  await expect(page.getByRole("heading", { name: "Load Game" })).toBeVisible();
  const box = await page.locator(".saves-panel").boundingBox();
  const size = page.viewportSize();
  if (!box || !size) throw new Error("saves panel is missing");
  expect(box.y).toBeGreaterThanOrEqual(0);
  expect(box.y + box.height).toBeLessThanOrEqual(size.height);
  expect(box.x + box.width).toBeLessThanOrEqual(size.width);
  const row = page.locator(`#saves-list li[data-save="${saveId}"]`);
  await expect(row).toBeVisible();
  const resume = row.getByRole("button", { name: "Resume E2E save" });
  try {
    // Send a real pointer click while startup is held, bypassing Playwright's
    // usual wait for an enabled button.
    await resume.click({ force: true });
    await expect(page.locator("#loading-error")).toBeHidden();
    await expect(resume).toBeDisabled({ timeout: 2_000 });
    await expect(page.locator("#saves-status")).toHaveText(
      "Connecting to your world…",
    );
  } finally {
    releaseClient();
  }
  await resume.click({ timeout: 60_000 });
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
  await row.getByRole("button", { name: "Delete E2E save" }).click();
  await expect(row).toHaveCount(0);
});
