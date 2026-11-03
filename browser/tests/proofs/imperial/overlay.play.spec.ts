import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { centerVillage } from "../../village/fixtures/resources.js";

// Run after assets-import-de without AOE_ASSET_PACK to inspect the attached
// world and actor categories. Licensed pixels and recordings stay local.
test.use({ video: { mode: "on", size: { width: 1280, height: 720 } } });
test("the imported overlay and its actors load from the trial pack", async ({
  page,
  request,
}, info) => {
  test.setTimeout(120_000);
  test.skip(
    !(await request.get("/asset-pack/manifest.json")).ok(),
    "Requires a privately imported trial pack",
  );
  test.skip(
    !(await request.get("/asset-pack/buildings/manifest.json")).ok(),
    "Requires an automatically attached DE overlay",
  );
  for (const category of ["buildings", "buildings/actors"]) {
    expect(
      (await request.get(`/asset-pack/${category}/manifest.json`)).ok(),
    ).toBe(true);
  }
  expect(
    (await request.post("/maps/reset?scenario=imperial-army")).status(),
  ).toBe(204);
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.goto("/?play=1#x=98&y=134&zoom=.5");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  const scene = page.locator("#scene");
  await expect(scene).toHaveAttribute("data-village-art", "native");
  await expect(scene).toHaveAttribute("data-chunks-pending", "0", {
    timeout: 60_000,
  });
  await expect(scene).toHaveAttribute("data-chunks-failed", "0");
  await centerVillage(page, true);
  await expect(page.locator("#selection-name")).toHaveText("Town Center");
  await page.screenshot({ path: info.outputPath("attached-overlay.png") });
  await page.waitForTimeout(1500); // Hold the loaded overlay in the proof video.
});
