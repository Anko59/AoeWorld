import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { completedJob, setup, skirmish } from "./skirmish.js";

// A reload during a running skirmish returns to it: no cinematic or menu,
// the loading screen through the terrain wait, then the game as controller.
test.beforeEach(async ({ page, request }) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
});

test("reloading a running skirmish resumes it as the controller", async ({
  page,
}) => {
  test.setTimeout(180_000);
  const hash = process.env["AOE_E2E_LANDSCAPE_HASH"];
  if (!hash) throw new Error("The E2E stack must supply a prepared map");
  const server = await skirmish(page, [
    { phase: "connecting_ai", completed: 1, total: 3, unit: "AI players" },
    {
      phase: "ready",
      completed: 3,
      total: 3,
      unit: "AI players",
      content_hash: hash,
    },
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
  await page.getByLabel("AI opponents").fill("3");
  await page.getByRole("button", { name: "Start Game", exact: true }).click();
  await expect(page.locator("#start-sequence")).toBeHidden({ timeout: 60_000 });
  await expect(page.locator("#connection")).toHaveText("connected");
  const token = await page.evaluate(() =>
    sessionStorage.getItem("aoeworld.resume-token"),
  );
  expect(token).toMatch(/^[0-9a-f]{48}$/i);

  // Every entry state the reloaded page passes through, from its first script.
  await page.addInitScript(() => {
    const seen: string[] = [];
    (window as unknown as { startStates: string[] }).startStates = seen;
    new MutationObserver(() => {
      const state = document.body?.dataset["startState"];
      if (state && seen.at(-1) !== state) seen.push(state);
    }).observe(document, {
      subtree: true,
      attributes: true,
      attributeFilter: ["data-start-state"],
    });
  });
  // Reload at the root path: this tab entered through ?skipIntro=1, so it
  // has not seen the cinematic and `/` alone would open it.
  await page.goto("/");
  await expect
    .poll(
      () =>
        page.evaluate(
          () => (window as unknown as { startStates: string[] }).startStates,
        ),
      { timeout: 60_000 },
    )
    .toContain("game");
  await expect(page.locator("#start-sequence")).toBeHidden();
  await expect(page.locator("#loading-title")).toHaveText(
    "Returning to your game",
  );
  await expect(page.locator("#minimap")).toHaveAttribute("data-map-hash", hash);
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect(page.locator("#observer-label")).toBeHidden();
  expect(
    await page.evaluate(() => sessionStorage.getItem("aoeworld.resume-token")),
  ).toBe(token);
  const states = await page.evaluate(
    () => (window as unknown as { startStates: string[] }).startStates,
  );
  expect(states).toEqual(["loading", "game"]);
  expect(server.stops).toHaveLength(0);

  // Quit to Main Menu still ends the match.
  await page.getByRole("button", { name: "Menu", exact: true }).click();
  await page.getByRole("button", { name: "Quit to Main Menu" }).click();
  await expect(page.locator("#main-menu")).toBeVisible();
  await expect.poll(() => server.stops.length).toBe(1);
});

test("a reload without a running match opens the menu as before", async ({
  page,
}) => {
  await skirmish(page, []);
  await setup(page);
  await page.reload();
  await expect(page.locator("#main-menu")).toBeVisible();
  await expect(page.locator("#entry-intro")).toBeHidden();
});
