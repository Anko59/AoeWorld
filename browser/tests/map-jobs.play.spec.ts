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

test("lost submission acknowledgement reuses the saved key and request after reload", async ({
  page,
}) => {
  await gameAssets(page);
  const submissions: { key: string | undefined; body: unknown }[] = [];
  let original: Record<string, unknown> = {};
  await page.route("**/maps/jobs", async (route) => {
    if (route.request().method() === "GET") {
      await route.fulfill({ json: [] });
      return;
    }
    original = route.request().postDataJSON();
    submissions.push({
      key: route.request().headers()["idempotency-key"],
      body: original,
    });
    if (submissions.length === 1) await route.abort("connectionfailed");
    else await route.fulfill({ json: { id: 79 } });
  });
  await page.route("**/maps/jobs/79", async (route) => {
    await route.fulfill({
      json: {
        id: 79,
        state: "cancelled",
        request: original,
        preparation: { mode: "procedural_fallback" },
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
  await expect(page.locator("#map-estimate")).toContainText(
    "Recover this request",
  );
  await expect(
    page.getByRole("button", { name: "Generate", exact: true }),
  ).toBeDisabled();
  await page.reload();
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-assets",
    "aoe2-local",
  );
  await page.getByRole("button", { name: "Open map creator" }).click();
  await page
    .getByRole("button", { name: "Recover request", exact: true })
    .click();
  await expect(page.locator("#map-estimate")).toContainText(
    "Map creation cancelled",
  );
  expect(submissions).toHaveLength(2);
  expect(submissions[0]?.key).toMatch(/^[0-9a-f]{32}$/);
  expect(submissions[1]).toEqual(submissions[0]);
  await expect(
    page.getByRole("button", { name: "Generate", exact: true }),
  ).toBeEnabled();
});

test("explicit submission conflict permits a fresh key instead of trapping recovery", async ({
  page,
}) => {
  await gameAssets(page);
  const keys: (string | undefined)[] = [];
  await page.route("**/maps/jobs", async (route) => {
    if (route.request().method() === "GET") await route.fulfill({ json: [] });
    else {
      keys.push(route.request().headers()["idempotency-key"]);
      await route.fulfill({
        status: 409,
        body: "submission key was already used for another request",
      });
    }
  });
  await page.goto("/");
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-assets",
    "aoe2-local",
  );
  await page.getByRole("button", { name: "Open map creator" }).click();
  const generate = page.getByRole("button", { name: "Generate", exact: true });
  await generate.click();
  await expect(page.locator("#map-estimate")).toContainText("already used");
  await expect(generate).toBeEnabled();
  await generate.click();
  await expect.poll(() => keys.length).toBe(2);
  expect(keys[0]).toMatch(/^[0-9a-f]{32}$/);
  expect(keys[1]).not.toBe(keys[0]);
});
