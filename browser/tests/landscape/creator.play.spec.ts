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
  await page.goto("/");
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
