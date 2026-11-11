import { expect } from "@playwright/test";
import { test } from "../../server.js";

test("world construction progress appears only in the detailed view", async ({
  page,
}) => {
  await page.route("**/construction-test", (route) =>
    route.fulfill({
      contentType: "text/html",
      body: '<link rel="stylesheet" href="/interface/game.css"><div id="construction-labels"><span style="left:300px;top:200px">50%<progress value="500" max="1000"></progress></span></div>',
    }),
  );
  await page.goto("/construction-test");
  for (const lod of ["sprites", "markers", "map", "sprites"]) {
    await page.evaluate((value) => {
      document.documentElement.dataset.lod = value;
    }, lod);
    if (lod === "sprites") await expect(page.locator("progress")).toBeVisible();
    else await expect(page.locator("progress")).toBeHidden();
  }
});
