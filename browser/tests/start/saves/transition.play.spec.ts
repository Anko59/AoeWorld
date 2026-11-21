import { expect } from "@playwright/test";
import { gameAssets } from "../../game-assets.js";
import { fallbackTest as test, forceWebGl } from "../../rendering/backends.js";
import { completedJob } from "../skirmish.js";
import { saveMatch } from "./flow.js";

test.beforeEach(async ({ page, request }) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
});

test("a loaded save zooms straight to the province on its first wheel-out", async ({
  page,
}) => {
  test.setTimeout(180_000);
  const hash = process.env["AOE_E2E_LANDSCAPE_HASH"];
  if (!hash) throw new Error("The E2E stack must supply a prepared map");
  await forceWebGl(page);
  await completedJob(page, hash);
  await page.goto("/?skipIntro=1");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await page
    .getByRole("button", { name: "Single Player", exact: true })
    .click();
  await saveMatch(page);

  await page.reload();
  await page.getByRole("button", { name: "Load Game" }).click();
  await page.getByRole("button", { name: "Resume E2E save" }).click();
  await expect(page.locator("#start-sequence")).toBeHidden({
    timeout: 120_000,
  });
  await expect(page.locator("#minimap")).toHaveAttribute("data-map-hash", hash);
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    "webgl2",
  );
  await expect(page.locator("#scene")).toHaveAttribute("data-lod", "sprites");

  // Drive the real wheel input so the loading path cannot bypass the reveal.
  const nearZoom = Number(
    await page.locator("#minimap").getAttribute("data-zoom"),
  );
  const viewport = page.viewportSize();
  if (!viewport || nearZoom <= 0.1) throw new Error("Near camera missing");
  await page.mouse.move(viewport.width / 2, viewport.height * 0.42);
  await page.mouse.wheel(0, Math.log(nearZoom / 0.08) / 0.0015);
  await expect(page.locator("#strategic-scene")).toBeVisible();
  expect(await page.locator("#cockpit-reveal").isHidden()).toBe(true);
  await expect(page.locator("#scene")).toHaveAttribute("data-lod", "markers");
});
