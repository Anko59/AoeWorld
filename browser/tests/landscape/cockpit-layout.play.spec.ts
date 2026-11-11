import { expect } from "@playwright/test";
import { test } from "../../server.js";

test("far-view hotkey keycaps fit one text line with original and fallback art", async ({
  page,
}) => {
  await page.route("**/keycap-test", (route) =>
    route.fulfill({
      contentType: "text/html",
      body: '<link rel="stylesheet" href="/interface/game.css"><div id="far-dock"><button class="dock-btn"><span class="icon"></span><kbd>F1</kbd></button></div>',
    }),
  );
  await page.goto("/keycap-test");
  for (const art of ["fallback", "original"]) {
    await page.evaluate((value) => {
      document.documentElement.dataset.farArt = value;
    }, art);
    const key = await page.locator("kbd").boundingBox();
    const button = await page.locator("button").boundingBox();
    if (!key || !button) throw new Error("Hotkey box is missing");
    // 8px glyphs: a 1px frame and no vertical padding make a 10px keycap.
    expect(key.height).toBeLessThanOrEqual(10.5);
    expect(key.height).toBeGreaterThanOrEqual(10);
    expect(key.y).toBeGreaterThan(button.y + button.height / 2);
  }
});
