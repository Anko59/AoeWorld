import { expect, test } from "@playwright/test";
import { gameAssets } from "../game-assets.js";

const cases = [
  { saved: { id: 987654 }, status: 404 },
  ...[403, 404, 409].map((status) => ({
    saved: {
      submission: { key: "a".repeat(32), request: { schema_version: 1 } },
    },
    status,
  })),
];
for (const { saved, status } of cases) {
  test(`stale saved request is probed once then dropped: ${status} ${JSON.stringify(saved)}`, async ({
    page,
    request,
  }) => {
    expect((await request.post("/maps/reset")).status()).toBe(204);
    await gameAssets(page);
    await page.addInitScript(
      (saved) =>
        localStorage.setItem(
          "aoeworld.map-creation-job",
          JSON.stringify(saved),
        ),
      saved,
    );
    const keys: (string | undefined)[] = [];
    await page.route("**/maps/jobs/987654", (route) =>
      route.fulfill({ status: 404, body: "Unknown job" }),
    );
    await page.route("**/maps/jobs", async (route) => {
      if (route.request().method() !== "POST")
        return route.fulfill({ json: [] });
      keys.push(route.request().headers()["idempotency-key"]);
      await route.fulfill({
        status,
        body:
          status === 403
            ? "Requires the current game controller"
            : "Unknown or refused submission",
      });
    });
    await page.goto("/?skipIntro=1");
    await page
      .getByRole("button", { name: "Single Player", exact: true })
      .click();
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-connected",
      "true",
    );
    await expect(
      page.getByRole("button", { name: "Start Game", exact: true }),
    ).toBeEnabled();
    await expect(page.locator("#setup-status")).not.toContainText("Connecting");
    expect(keys).toEqual("submission" in saved ? [saved.submission.key] : []);
    expect(
      await page.evaluate(() =>
        localStorage.getItem("aoeworld.map-creation-job"),
      ),
    ).toBeNull();
    await expect(page.locator("#map-creator, #creator-toggle")).toHaveCount(0);
    if ("submission" in saved) {
      await page
        .getByRole("button", { name: "Start Game", exact: true })
        .click();
      await expect(page.locator("#loading-error")).toBeVisible();
      expect(keys).toHaveLength(2);
      expect(keys[1]).toMatch(/^[0-9a-f]{32}$/);
      expect(keys[1]).not.toBe(keys[0]);
    }
  });
}

test("stalled startup recovery releases setup after ten seconds and keeps the original key", async ({
  page,
  request,
}) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
  const key = "b".repeat(32);
  await page.addInitScript(
    (saved) =>
      localStorage.setItem("aoeworld.map-creation-job", JSON.stringify(saved)),
    { submission: { key, request: { schema_version: 1 } } },
  );
  const keys: (string | undefined)[] = [];
  const stalled: unknown[] = [];
  await page.route("**/maps/jobs", async (route) => {
    if (route.request().method() !== "POST") return route.fulfill({ json: [] });
    keys.push(route.request().headers()["idempotency-key"]);
    // The first probe is held open and never answered, so only the client's
    // recovery timeout can release setup. The retry receives a retryable 500.
    if (keys.length === 1) return void stalled.push(route);
    await route.fulfill({ status: 500, body: "Retry later" });
  });
  await page.goto("/?skipIntro=1");
  await page
    .getByRole("button", { name: "Single Player", exact: true })
    .click();
  const start = page.getByRole("button", {
    name: /^(Start Game|Resume map preparation)/,
  });
  await expect.poll(() => keys.length, { timeout: 30_000 }).toBe(1);
  await expect(start).toBeDisabled();
  // Recovery gives up after the stated ten seconds; allow two seconds of slack.
  await expect(start).toBeEnabled({ timeout: 12_000 });
  expect(keys).toEqual([key]);
  expect(
    await page.evaluate(() =>
      localStorage.getItem("aoeworld.map-creation-job"),
    ),
  ).toContain(key);
  await start.click();
  await expect.poll(() => keys.length).toBe(2);
  expect(keys[1]).toBe(key);
  await expect(page.locator("#loading-error")).toBeVisible();
});
