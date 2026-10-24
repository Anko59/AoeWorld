import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { completedJob, setup, skirmish } from "./skirmish.js";

// The AI opponents setting and the match phases of loading. The E2E server
// has no AI host, so the skirmish endpoints are scripted here with a small
// opponent count; the map, its activation and the terrain wait are real.
test.beforeEach(async ({ page, request }) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
});

test("the France preset is a whole country and opponents need an AI server", async ({
  page,
}) => {
  await setup(page);
  // This E2E server has no AI host: the setting says so and plays alone.
  await expect(page.getByLabel("AI opponents")).toBeDisabled();
  await expect(page.locator("#opponents-note")).toContainText("make dev");
  await page.getByRole("button", { name: "France", exact: true }).click();
  await expect(page.getByLabel("Starting latitude")).toHaveValue("46.6");
  await expect(page.getByLabel("Starting longitude")).toHaveValue("2.5");
  await expect(
    page.getByRole("combobox", { name: "Map size", exact: true }),
  ).toHaveValue("960");
  await expect(page.locator("#skirmish-estimate")).toContainText(
    "960 km of real landscape",
  );
  await expect(page.locator("#chosen-location")).toHaveText("France");
});

test("AI opponents: loading shows village and seat counts, then opens the game", async ({
  page,
}) => {
  const hash = process.env["AOE_E2E_LANDSCAPE_HASH"];
  if (!hash) throw new Error("The E2E stack must supply a prepared map");
  const server = await skirmish(page, [
    { phase: "opening_map" },
    { phase: "placing_villages", completed: 2, total: 4, unit: "villages" },
    { phase: "placing_villages", completed: 2, total: 4, unit: "villages" },
    { phase: "connecting_ai", completed: 1, total: 3, unit: "AI players" },
    { phase: "connecting_ai", completed: 1, total: 3, unit: "AI players" },
    { phase: "ready", completed: 3, total: 3, unit: "AI players" },
  ]);
  // The real server swaps in the map while the scripted phases run.
  server.setOnPost(async (route) => {
    const token =
      route.request().headers()["x-aoeworld-controller-token"] ?? "";
    const activated = await page.request.post(`/maps/${hash}`, {
      headers: { "X-AoeWorld-Controller-Token": token },
    });
    expect(activated.ok()).toBeTruthy();
  });
  await completedJob(page, hash);
  await setup(page);
  const opponents = page.getByLabel("AI opponents");
  await expect(opponents).toBeEnabled();
  await expect(opponents).toHaveValue("7");
  await opponents.fill("3");
  await page.getByRole("button", { name: "Start Game", exact: true }).click();
  await expect(page.locator("#loading-screen")).toBeVisible();
  await expect(page.locator("#loading-location")).toContainText(
    "3 AI opponents",
  );
  await expect(page.locator("#loading-phase")).toContainText("2 / 4 villages");
  await expect(page.getByRole("progressbar")).toHaveAttribute(
    "aria-valuemax",
    "4",
  );
  await expect(page.locator("#loading-phase")).toContainText(
    "1 / 3 AI players",
  );
  await expect(page.locator("#loading-cancel")).toBeEnabled();
  expect(server.posted).toEqual([{ map_hash: hash, opponents: 3 }]);
  // Ready hands over to the terrain wait; loading stays until it is playable.
  await expect(page.locator("#start-sequence")).toBeHidden({ timeout: 60_000 });
  await expect(page.locator("#minimap")).toHaveAttribute("data-map-hash", hash);
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-chunks-pending",
    "0",
  );
  // Quit to Main Menu stops the AI players.
  await page.getByRole("button", { name: "Menu", exact: true }).click();
  await page.getByRole("button", { name: "Quit to Main Menu" }).click();
  await expect(page.locator("#main-menu")).toBeVisible();
  await expect.poll(() => server.stops.length).toBe(1);
  expect(server.stops[0]).toMatch(/^[0-9a-f]{48}$/i);
});

test("Cancel while AI players join stops them and returns to setup", async ({
  page,
}) => {
  const hash = process.env["AOE_E2E_LANDSCAPE_HASH"];
  if (!hash) throw new Error("The E2E stack must supply a prepared map");
  const server = await skirmish(page, [
    { phase: "connecting_ai", completed: 0, total: 2, unit: "AI players" },
  ]);
  await completedJob(page, hash);
  await setup(page);
  await page.getByLabel("AI opponents").fill("2");
  await page.getByRole("button", { name: "Start Game", exact: true }).click();
  await expect(page.locator("#loading-phase")).toContainText(
    "0 / 2 AI players",
  );
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Game Setup" })).toBeVisible();
  expect(server.stops).toHaveLength(1);
  await expect(page.getByLabel("AI opponents")).toHaveValue("2");
});
