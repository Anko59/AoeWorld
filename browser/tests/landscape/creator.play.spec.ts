import { expect, test } from "@playwright/test";
import { gameAssets } from "../game-assets.js";

test("creator keeps the published default and submits the explicit composed preview", async ({
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
  const profile = page.getByRole("combobox", { name: "Landscape profile" });
  await expect(profile).toHaveValue("standard_v1");
  await profile.selectOption("landscape_v2");
  await page.getByRole("button", { name: "Generate", exact: true }).click();
  await expect.poll(() => submitted?.detail_profile).toBe("landscape_v2");
  expect(submitted?.schema_version).toBe(1);
  expect(submitted?.year_ce).toBe(600);
  await expect(page.locator("#map-estimate")).toContainText(
    "Map creation cancelled",
  );
});
