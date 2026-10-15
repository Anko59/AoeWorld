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
  await page.getByRole("button", { name: "Menu", exact: true }).click();
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
    await page.getByRole("button", { name: "Menu", exact: true }).click();
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

test("map creator pans, zooms, and preserves preview-only fallback maps", async ({
  page,
  request,
}) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-assets",
    "aoe2-local",
  );
  await page.getByRole("button", { name: "Open map creator" }).click();
  await expect(page.getByLabel("Region")).toHaveValue("paris");
  await expect(page.locator("#map-coordinates")).toBeHidden();
  await expect(page.locator("#map-location")).toContainText("Paris Basin");
  await expect(page.locator("#map-footprint")).toHaveAttribute(
    "visibility",
    "visible",
  );
  await expect(page.locator("#map-footprint")).toHaveAttribute("d", /M/);
  const overview = page.locator("#map-overview");
  const overviewBox = await overview.boundingBox();
  if (!overviewBox) throw new Error("world overview is missing");
  await overview.hover({
    position: { x: overviewBox.width / 2, y: overviewBox.height / 2 },
  });
  await page.mouse.wheel(0, -120);
  await expect(overview).not.toHaveAttribute("viewBox", "0 0 360 180");
  const zoomedView = await overview.getAttribute("viewBox");
  if (!zoomedView) throw new Error("zoom did not set an overview viewBox");
  await page.keyboard.down("Shift");
  await page.mouse.move(
    overviewBox.x + overviewBox.width / 2,
    overviewBox.y + overviewBox.height / 2,
  );
  await page.mouse.down();
  await page.mouse.move(
    overviewBox.x + overviewBox.width / 2 + 24,
    overviewBox.y + overviewBox.height / 2,
  );
  await page.mouse.up();
  await page.keyboard.up("Shift");
  await expect(overview).not.toHaveAttribute("viewBox", zoomedView);
  await overview.dblclick({
    position: { x: overviewBox.width / 2, y: overviewBox.height / 2 },
  });
  await page.mouse.move(
    overviewBox.x + overviewBox.width * 0.72,
    overviewBox.y + overviewBox.height * 0.32,
  );
  await page.mouse.down();
  await page.mouse.move(
    overviewBox.x + overviewBox.width * 0.74,
    overviewBox.y + overviewBox.height * 0.34,
  );
  await page.mouse.up();
  await expect(page.getByLabel("Region")).toHaveValue("custom");
  await expect(page.locator("#map-coordinates")).toBeVisible();
  await page.getByLabel("Region").selectOption("nile");
  await expect(page.locator("#map-location")).toContainText("Nile Delta");
  await page.getByLabel("Region").selectOption("fiji");
  await expect(page.locator("#map-location")).toContainText("Fiji");
  await expect(page.getByLabel("Latitude", { exact: true })).toHaveValue(
    "-17.8",
  );
  await expect(page.getByLabel("Longitude", { exact: true })).toHaveValue(
    "179.8",
  );
  await page.getByLabel("Region").selectOption("antarctic");
  await expect(page.locator("#map-location")).toContainText(
    "Antarctic interior",
  );
  await page.getByLabel("Region").selectOption("custom");
  await expect(page.locator("#map-coordinates")).toBeVisible();
  await page.getByLabel("Latitude", { exact: true }).fill("48.8566");
  await page.getByLabel("Longitude", { exact: true }).fill("2.3522");
  await page.getByLabel("Square km").fill("0.256");
  await page.getByLabel("Compression", { exact: true }).fill("2");
  await page.getByRole("button", { name: "Estimate" }).click();
  await expect(page.locator("#map-estimate")).toContainText("64 × 64 tiles");
  await expect(page.locator("#map-estimate")).toContainText(
    "procedural fallback",
  );
  await page.getByLabel("Terrain detail").selectOption("detailed");
  await page.getByRole("button", { name: "Estimate" }).click();
  await expect(page.locator("#map-estimate")).toContainText(
    "requires a configured geographic worker",
  );
  await expect(page.getByLabel("Rivers")).toBeDisabled();
  await expect(page.getByLabel("Rivers")).toHaveValue("vectors");
  await page.getByLabel("Terrain detail").selectOption("automatic");
  await expect(page.getByLabel("Rivers")).toBeEnabled();
  await expect(page.getByLabel("Rivers")).toHaveValue("automatic");
  await expect(page.getByRole("button", { name: "Generate" })).toBeEnabled();
  // The old Paris fallback now has a start. This valid 64×64 polar fallback
  // has no passable tiles (also certified by the native fixture regression).
  await page.getByLabel("Latitude", { exact: true }).fill("-79.1524");
  await page.getByLabel("Longitude", { exact: true }).fill("0");
  await page.getByRole("button", { name: "Estimate" }).click();
  await expect(page.locator("#map-estimate")).toContainText("64 × 64 tiles");
  const activeHash = await page
    .locator("#minimap")
    .getAttribute("data-map-hash");
  const generated = page.waitForResponse(
    (response) =>
      /^\/maps\/[0-9a-f]{64}$/.test(new URL(response.url()).pathname) &&
      response.request().method() === "POST",
  );
  await page.getByRole("button", { name: "Generate" }).click();
  await expect(page.locator("#map-estimate")).toContainText(
    "Fallback package available for preview",
  );
  await expect(page.locator("#map-estimate")).toContainText(
    "no suitable land start.",
  );
  const result = await (await generated).json();
  expect(result.start_available).toBe(false);
  expect(result.tiles_per_side).toBe(64);
  await page
    .getByRole("combobox", { name: "Saved maps", exact: true })
    .selectOption(result.content_hash);
  await page
    .getByRole("button", { name: "Preview saved", exact: true })
    .click();
  await expect(page.locator("#terrain-preview-summary")).toContainText(
    "Fallback coarse",
  );
  await expect(page.locator("#terrain-preview-canvas")).toBeVisible();
  await expect(page.locator("#connection")).toHaveText("connected");
  expect(await page.locator("#minimap").getAttribute("data-map-hash")).toBe(
    activeHash,
  );
});
