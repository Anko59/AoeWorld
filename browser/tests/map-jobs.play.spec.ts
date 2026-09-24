import { expect, test } from "@playwright/test";
import { gameAssets } from "./game-assets.js";

test.beforeEach(async ({ request }) => {
  const response = await request.post("/maps/reset");
  expect(response.status()).toBe(204);
});

test("creator shows measured phase counts without an invented overall percentage", async ({
  page,
}) => {
  await gameAssets(page);
  let original: Record<string, unknown> = {};
  let phase = "building_pyramids";
  let complete = false;
  await page.route("**/maps/jobs", async (route) => {
    if (route.request().method() === "GET") {
      await route.fulfill({ json: [] });
    } else {
      original = route.request().postDataJSON();
      await route.fulfill({ json: { id: 78 } });
    }
  });
  await page.route("**/maps/jobs/78", async (route) => {
    await route.fulfill({
      json: {
        id: 78,
        request: original,
        state: complete ? "cancelled" : "running",
        stage: "preparing_detailed",
        percent: null,
        eta_seconds: null,
        preparation: {
          mode: "detailed",
          explanation: "Regional source preparation",
        },
        progress: {
          version: 1,
          phase,
          completed: phase === "building_pyramids" ? 4 : null,
          total: phase === "building_pyramids" ? 16 : null,
          unit: phase === "building_pyramids" ? "pages" : null,
        },
      },
    });
  });
  await page.goto("/");
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-assets",
    "aoe2-local",
  );
  await page.getByRole("button", { name: "Open map creator" }).click();
  await page.getByRole("button", { name: "Generate", exact: true }).click();
  const output = page.locator("#map-estimate");
  await expect(output).toContainText("building pyramids · 4 / 16 pages");
  await expect(output).not.toContainText("%");
  await expect(output).not.toContainText("ETA");
  phase = "verifying_package";
  await expect(output).toContainText("verifying package");
  await expect(output).not.toContainText("4 / 16");
  complete = true;
  await expect(output).toContainText("Map creation cancelled");
  await expect(
    page.getByRole("button", { name: "Generate", exact: true }),
  ).toBeEnabled();
});
