import { expect, test } from "@playwright/test";
import { gameAssets } from "../game-assets.js";

test("creator submits the single map request shape without a detail profile", async ({
  page,
  request,
}) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
  let submitted: Record<string, unknown> | undefined;
  await page.route("**/maps/jobs", async (route) => {
    if (route.request().method() === "GET") await route.fulfill({ json: [] });
    else {
      submitted = route.request().postDataJSON();
      await route.fulfill({ json: { id: 807 } });
    }
  });
  await page.route("**/maps/jobs/807", async (route) =>
    route.fulfill({
      json: {
        id: 807,
        request: submitted,
        state: "cancelled",
        preparation: { mode: "procedural_fallback" },
      },
    }),
  );
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected");
  await page.getByRole("button", { name: "Open map creator" }).click();
  await expect(
    page.getByRole("combobox", { name: "Landscape profile" }),
  ).toHaveCount(0);
  await page.getByRole("button", { name: "Generate", exact: true }).click();
  await expect.poll(() => submitted?.schema_version).toBe(1);
  expect(submitted).not.toHaveProperty("detail_profile");
  expect(submitted?.year_ce).toBe(600);
  await expect(page.locator("#map-estimate")).toContainText(
    "Map creation cancelled",
  );
});

for (const [region, outside] of [
  ["paris", false],
  ["fiji", true],
] as const) {
  test(`automatic rivers preserve the ${region} creator request and coverage note`, async ({
    page,
    request,
  }) => {
    expect((await request.post("/maps/reset")).status()).toBe(204);
    await gameAssets(page);
    let submitted: Record<string, unknown> | undefined;
    const hash = "a".repeat(64);
    await page.route("**/maps/jobs", async (route) => {
      if (route.request().method() === "GET") await route.fulfill({ json: [] });
      else {
        submitted = route.request().postDataJSON();
        await route.fulfill({ status: 202, json: { id: 808 } });
      }
    });
    await page.route("**/maps/jobs/808", async (route) =>
      route.fulfill({
        json: {
          id: 808,
          request: submitted,
          state: "completed",
          content_hash: hash,
          preparation: { mode: "overview", hydrology_mode: "automatic" },
          preparation_notes: outside ? ["outside_river_coverage"] : [],
        },
      }),
    );
    await page.route(`**/maps/${hash}`, async (route) =>
      route.fulfill({
        json: {
          content_hash: hash,
          start_available: false,
          uses_fallback_data: false,
          tiles_per_side: 512,
          source_lock_count: outside ? 7 : 9,
          message: "No suitable land start.",
        },
      }),
    );
    await page.goto("/?play=1");
    await expect(page.locator("#connection")).toHaveText("connected");
    await page.getByRole("button", { name: "Open map creator" }).click();
    await page.getByLabel("Region").selectOption(region);
    await expect(page.getByLabel("Rivers", { exact: true })).toHaveValue(
      "automatic",
    );
    await page.getByRole("button", { name: "Generate", exact: true }).click();
    await expect.poll(() => submitted?.["hydrology_mode"]).toBe("automatic");
    await expect(page.locator("#map-estimate")).toContainText(
      "available for preview",
    );
    if (outside)
      await expect(page.locator("#map-estimate")).toContainText(
        "outside river coverage",
      );
    else
      await expect(page.locator("#map-estimate")).not.toContainText(
        "outside river coverage",
      );
  });
}
