import { expect, test } from "@playwright/test";
import { gameAssets } from "../game-assets.js";

for (const saved of [
  { id: 987654 },
  { submission: { key: "a".repeat(32), request: { schema_version: 1 } } },
]) {
  test(`stale saved map request is dropped: ${JSON.stringify(saved)}`, async ({
    page,
    request,
  }) => {
    expect((await request.post("/maps/reset")).status()).toBe(204);
    await gameAssets(page);
    await page.addInitScript((saved) => {
      localStorage.setItem("aoeworld.map-creation-job", JSON.stringify(saved));
    }, saved);
    await page.route("**/maps/jobs/987654", (route) =>
      route.fulfill({ status: 404, body: "Unknown job" }),
    );
    await page.route("**/maps/jobs", (route) =>
      route.request().method() === "POST"
        ? route.fulfill({
            status: 403,
            body: "Requires the current game controller",
          })
        : route.fulfill({ json: [] }),
    );
    await page.goto("/?skipIntro=1");
    await page
      .getByRole("button", { name: "Single Player", exact: true })
      .click();
    await expect(page.locator("#connection")).toHaveText("connected");
    await expect(
      page.getByRole("button", { name: "Start Game", exact: true }),
    ).toBeEnabled();
    await expect(page.locator("#setup-status")).not.toContainText("Connecting");
    expect(
      await page.evaluate(() =>
        localStorage.getItem("aoeworld.map-creation-job"),
      ),
    ).toBeNull();
    await expect(page.locator("#map-creator, #creator-toggle")).toHaveCount(0);
  });
}
