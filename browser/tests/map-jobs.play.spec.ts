import { expect, test } from "@playwright/test";
import { gameAssets } from "./game-assets.js";

test.beforeEach(async ({ page, request }) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
});

test("lost submission acknowledgement reuses the saved key and request after reload", async ({
  page,
}) => {
  const submissions: { key: string | undefined; body: unknown }[] = [];
  let cancelled = false;
  await page.route("**/maps/jobs", async (route) => {
    if (route.request().method() === "GET") return route.fulfill({ json: [] });
    submissions.push({
      key: route.request().headers()["idempotency-key"],
      body: route.request().postDataJSON(),
    });
    // Acceptance happened, but its acknowledgement never reached this tab.
    if (submissions.length === 1) return route.abort("connectionfailed");
    await route.fulfill({ json: { id: 79 } });
  });
  await page.route(/\/maps\/jobs\/79(?:\/cancel)?$/, async (route) => {
    if (route.request().url().endsWith("/cancel")) cancelled = true;
    await route.fulfill({
      json: {
        id: 79,
        state: cancelled ? "cancelled" : "running",
        request: submissions[0]?.body,
        preparation: { mode: "overview" },
        progress: {
          phase: "building_pyramids",
          completed: 4,
          total: 16,
          unit: "pages",
        },
      },
    });
  });
  await page.goto("/?skipIntro=1");
  await page
    .getByRole("button", { name: "Single Player", exact: true })
    .click();
  await page.getByRole("button", { name: "Start Game", exact: true }).click();
  await expect(page.locator("#loading-error")).toBeVisible();
  expect(submissions).toHaveLength(1);
  await page.reload();
  await page
    .getByRole("button", { name: "Single Player", exact: true })
    .click();
  await expect.poll(() => submissions.length).toBe(2);
  await page
    .getByRole("button", { name: "Resume map preparation", exact: true })
    .click();
  await expect(page.locator("#loading-screen")).toBeVisible();
  await expect(page.locator("#loading-phase")).toContainText("4 / 16 pages");
  await expect(page.locator("#loading-cancel")).toBeEnabled();
  expect(submissions[0]?.key).toMatch(/^[0-9a-f]{32}$/);
  expect(submissions[1]).toEqual(submissions[0]);
  await page.locator("#loading-cancel").click();
  await expect(
    page.getByRole("button", { name: "Start Game", exact: true }),
  ).toBeEnabled();
  expect(cancelled).toBe(true);
  expect(submissions).toHaveLength(2);
  expect(
    await page.evaluate(() =>
      localStorage.getItem("aoeworld.map-creation-job"),
    ),
  ).toBeNull();
});

test("an uncertain recovery response retains the original key for the next attempt", async ({
  page,
}) => {
  const key = "b".repeat(32);
  await page.addInitScript(
    (key) =>
      localStorage.setItem(
        "aoeworld.map-creation-job",
        JSON.stringify({ submission: { key, request: { schema_version: 1 } } }),
      ),
    key,
  );
  const keys: (string | undefined)[] = [];
  await page.route("**/maps/jobs", async (route) => {
    keys.push(route.request().headers()["idempotency-key"]);
    await route.fulfill({ status: 503, body: "Temporarily unavailable" });
  });
  await page.goto("/?skipIntro=1");
  await page
    .getByRole("button", { name: "Single Player", exact: true })
    .click();
  await expect.poll(() => keys.length).toBe(1);
  await page
    .getByRole("button", { name: "Resume map preparation", exact: true })
    .click();
  await expect.poll(() => keys.length).toBe(2);
  expect(keys).toEqual([key, key]);
  await expect(page.locator("#loading-error")).toContainText(
    "Temporarily unavailable",
  );
});
