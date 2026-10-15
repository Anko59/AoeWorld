import { expect, test, type Page } from "@playwright/test";
import { gameAssets } from "../game-assets.js";

test.beforeEach(async ({ page, request }) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
});

async function jobs(
  page: Page,
  completeFirst = false,
  completeCancellation = true,
) {
  const requests: Record<string, unknown>[] = [];
  const cancelled: number[] = [];
  const hash = process.env["AOE_E2E_LANDSCAPE_HASH"];
  if (!hash) throw new Error("The E2E stack must supply a prepared map");
  await page.route("**/maps/jobs", async (route) => {
    if (route.request().method() === "GET") return route.fulfill({ json: [] });
    requests.push(route.request().postDataJSON());
    await route.fulfill({ json: { id: 100 + requests.length } });
  });
  await page.route(/\/maps\/jobs\/\d+(?:\/cancel)?$/, async (route) => {
    const id = Number(new URL(route.request().url()).pathname.split("/")[3]);
    if (route.request().url().endsWith("/cancel")) {
      expect(route.request().method()).toBe("POST");
      expect(route.request().headers()["x-aoeworld-controller-token"]).toMatch(
        /^[0-9a-f]{48}$/i,
      );
      cancelled.push(id);
    }
    await route.fulfill({
      json: {
        id,
        request: requests[id - 101],
        state: cancelled.includes(id)
          ? completeCancellation
            ? "cancelled"
            : "cancel_requested"
          : completeFirst && id === 101
            ? "completed"
            : "running",
        content_hash: hash,
        stage: "preparing_source",
        preparation: {
          mode: "overview",
          explanation: "Preparing the landscape",
        },
        progress: {
          phase: "building_pyramids",
          completed: 4,
          total: 16,
          unit: "pages",
        },
      },
    });
  });
  return {
    requests,
    cancelled,
    finishCancellation: () => {
      completeCancellation = true;
    },
  };
}

async function setup(page: Page) {
  await page.goto("/?skipIntro=1");
  await page
    .getByRole("button", { name: "Single Player", exact: true })
    .click();
  await expect(page.getByRole("heading", { name: "Game Setup" })).toBeVisible();
  await expect(page.locator("#connection")).toHaveText("connected");
}

test("creator Generate after a skirmish keeps the playground and its Cancel usable", async ({
  page,
}) => {
  const server = await jobs(page, true);
  await setup(page);
  await page.getByRole("button", { name: "Start Game", exact: true }).click();
  await expect(page.locator("#start-sequence")).toBeHidden();
  await expect(page.locator("#scene")).toBeVisible();
  await page.getByRole("button", { name: "Open map creator" }).click();
  await page.getByRole("button", { name: "Generate", exact: true }).click();
  await expect(page.locator("#map-estimate")).toContainText("4 / 16 pages");
  await expect(page.locator("#playground")).toBeVisible();
  await expect(page.locator("#playground")).toHaveJSProperty("inert", false);
  await expect(page.locator("#loading-screen")).toBeHidden();
  const cancel = page.locator("#cancel-map");
  await expect(cancel).toBeVisible();
  await expect(cancel).toBeEnabled();
  await cancel.click();
  await expect.poll(() => server.cancelled).toEqual([102]);
  await expect(page.locator("#map-estimate")).toContainText(
    "Map creation cancelled",
  );
  await expect(
    page.getByRole("button", { name: "Generate", exact: true }),
  ).toBeEnabled();
  await expect(page.locator("#start-sequence")).toBeHidden();
  await expect(page.locator("#playground")).toBeVisible();
  await expect(page.locator("#playground")).toHaveJSProperty("inert", false);
});

test("startup Cancel cancels the accepted job and restores the skirmish choices", async ({
  page,
}) => {
  const server = await jobs(page, false, false);
  await setup(page);
  await page.getByRole("radio", { name: "Orange", exact: true }).check();
  await page.getByRole("button", { name: "Alps", exact: true }).click();
  await page.getByRole("button", { name: "Start Game", exact: true }).click();
  const loading = page.locator("#loading-screen");
  await expect(loading).toBeVisible();
  const cancel = loading.getByRole("button", { name: "Cancel", exact: true });
  await expect(cancel).toBeEnabled();
  await cancel.click();
  await expect.poll(() => server.cancelled).toEqual([101]);
  await expect(loading).toBeVisible();
  await expect(cancel).toBeDisabled();
  server.finishCancellation();
  await expect(page.getByRole("heading", { name: "Game Setup" })).toBeVisible();
  await expect(loading).toBeHidden();
  await expect(
    page.getByRole("radio", { name: "Orange", exact: true }),
  ).toBeChecked();
  await expect(page.getByLabel("Starting latitude")).toHaveValue("46.6");
  await expect(
    page.getByRole("button", { name: "Start Game", exact: true }),
  ).toBeEnabled();
  await expect
    .poll(() =>
      page.evaluate(() => localStorage.getItem("aoeworld.map-creation-job")),
    )
    .toBeNull();
  expect(server.requests).toHaveLength(1);
});

test("a rejected startup cancellation stays visible and can be retried", async ({
  page,
}) => {
  const server = await jobs(page);
  let attempts = 0;
  await page.route("**/maps/jobs/101/cancel", async (route) => {
    if (++attempts === 1) {
      await route.fulfill({
        status: 503,
        body: "Cancellation service temporarily unavailable.",
      });
    } else await route.fallback();
  });
  await setup(page);
  await page.getByRole("button", { name: "Start Game", exact: true }).click();
  const loading = page.locator("#loading-screen");
  const cancel = loading.getByRole("button", { name: "Cancel", exact: true });
  await cancel.click();
  await expect(page.locator("#loading-error")).toContainText(
    "Cancellation service temporarily unavailable.",
  );
  await expect(loading).toBeVisible();
  await expect(cancel).toBeEnabled();
  await cancel.click();
  await expect.poll(() => server.cancelled).toEqual([101]);
  await expect(page.getByRole("heading", { name: "Game Setup" })).toBeVisible();
  expect(attempts).toBe(2);
});
