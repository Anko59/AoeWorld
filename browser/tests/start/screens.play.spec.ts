import { expect, test } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { gameAssets } from "../game-assets.js";

for (const viewport of [
  { width: 1440, height: 900 },
  { width: 1280, height: 720 },
]) {
  test(`entry screens fit ${viewport.width} by ${viewport.height} and activate prepared terrain`, async ({
    page,
    request,
  }, info) => {
    test.setTimeout(90_000);
    expect((await request.post("/maps/reset")).status()).toBe(204);
    await gameAssets(page);
    await page.setViewportSize(viewport);
    const directory = new URL("../../../.cache/start-shots/", import.meta.url);
    await mkdir(directory, { recursive: true });
    const shot = async (name: string) =>
      page.screenshot({
        animations: "disabled",
        path: fileURLToPath(
          new URL(
            `${viewport.width}x${viewport.height}-${info.project.name}-${name}.png`,
            directory,
          ),
        ),
      });
    await page.goto("/?introStills=1");
    await page.evaluate(() => document.fonts.ready);
    await expect(page.locator("#entry-intro")).toBeVisible();
    await shot("intro-opening");
    await expect(page.locator("#entry-intro")).toHaveClass(/reveal/, {
      timeout: 17_000,
    });
    await expect
      .poll(() =>
        page
          .locator(".intro-title")
          .evaluate((element) => Number(getComputedStyle(element).opacity)),
      )
      .toBeGreaterThan(0.99);
    await shot("intro-title");
    await expect(page.locator("#main-menu")).toBeVisible();
    await shot("menu");
    await page
      .getByRole("button", { name: "Single Player", exact: true })
      .click();
    await page.getByRole("radio", { name: "Purple", exact: true }).check();
    await page
      .getByRole("button", { name: "Paris basin", exact: true })
      .click();
    await expect(page.locator("#skirmish-estimate")).toContainText(
      "real landscape",
    );
    const size = page.getByRole("combobox", { name: "Map size", exact: true });
    await expect(size).toHaveValue("480");
    await expect(size.locator("option")).toHaveText([
      "Tiny · 30 km square",
      "Small · 120 km square",
      "Large · 480 km square",
      "Giant · 1,920 km square",
    ]);
    await expect(page.locator("#skirmish-estimate")).toContainText(
      "compressed to 16 km in game",
    );
    await size.selectOption("30");
    await expect(page.locator("#skirmish-estimate")).toContainText(
      "compressed to 1 km in game",
    );
    await size.selectOption("480");
    await expect(page.locator("#skirmish-estimate")).toContainText(
      "480 km of real landscape per side, compressed to 16 km in game",
    );
    const panel = await page.locator(".walnut-panel").boundingBox();
    if (!panel) throw new Error("setup panel is missing");
    expect(panel.y).toBeGreaterThanOrEqual(0);
    expect(panel.y + panel.height).toBeLessThanOrEqual(viewport.height);
    expect(panel.x).toBeGreaterThanOrEqual(0);
    expect(panel.x + panel.width).toBeLessThanOrEqual(viewport.width);
    await shot("setup");
    const hash = process.env["AOE_E2E_LANDSCAPE_HASH"];
    if (!hash) throw new Error("The E2E stack must supply a prepared map");
    let original: Record<string, unknown> = {};
    let polls = 0;
    await page.route("**/maps/jobs", async (route) => {
      if (route.request().method() === "GET")
        return route.fulfill({ json: [] });
      original = route.request().postDataJSON();
      await route.fulfill({ json: { id: 93 } });
    });
    await page.route("**/maps/jobs/93", async (route) => {
      await route.fulfill({
        json: {
          id: 93,
          request: original,
          state: ++polls > 4 ? "completed" : "running",
          content_hash: hash,
          stage: "preparing_source",
          preparation: { mode: "overview" },
          progress: {
            phase: "building_pyramids",
            completed: polls,
            total: 4,
            unit: "pages",
          },
        },
      });
    });
    await expect(
      page.getByRole("button", { name: "Start Game", exact: true }),
    ).toBeEnabled();
    const activation = page.waitForResponse(
      (response) =>
        new URL(response.url()).pathname === `/maps/${hash}` &&
        response.request().method() === "POST",
    );
    const started = Date.now();
    await page.getByRole("button", { name: "Start Game", exact: true }).click();
    await expect(page.locator("#loading-screen")).toBeVisible();
    await shot("loading");
    await expect(page.locator("#start-sequence")).toBeHidden({
      timeout: 60_000,
    });
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-chunks-pending",
      "0",
    );
    await expect(page.locator("#minimap")).toHaveAttribute(
      "data-raster",
      "ready",
    );
    const response = await activation;
    await response.finished();
    const timing = response.request().timing();
    await info.attach("prepared-terrain-timing", {
      contentType: "application/json",
      body: JSON.stringify({
        activation_response_ms: timing.responseEnd,
        activation_to_visible_ms: Date.now() - timing.startTime,
        start_game_to_visible_ms: Date.now() - started,
      }),
    });
    await shot("game");
  });
}
