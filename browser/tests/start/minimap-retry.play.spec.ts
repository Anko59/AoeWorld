import { expect } from "@playwright/test";
import { test } from "../../server.js";

for (const resumed of [false, true]) {
  test(`${resumed ? "resumed" : "new"} match recovers after its minimap times out`, async ({
    page,
  }) => {
    await page.route("**/start/bootstrap.js", (route) =>
      route.fulfill({
        contentType: "application/javascript",
        body: `
        import { initInterface } from '/interface/hud.js';
        import { initNavigation } from '/navigation.js';
        import { initStartSequence } from '/start/sequence.js';
        initInterface(); initNavigation();
        window.aoeStart = initStartSequence({ readJson: r => r.json(), start() {}, cancel() {} });
      `,
      }),
    );
    if (resumed)
      await page.addInitScript(() =>
        localStorage.setItem("aoeworld.resume-token", "a".repeat(48)),
      );
    let requests = 0;
    await page.route("**/maps/*/minimap", (route) => {
      requests++;
      if (requests === 1) return; // Initial response stays pending past the request deadline.
      return route.fulfill({
        contentType: "application/octet-stream",
        body: Buffer.from([65, 87, 77, 77, 2, 0, 0, 0, 0, 0, 0, 0]),
      });
    });
    await page.clock.install();
    await page.goto("/?skipIntro=1");
    if (!resumed)
      await page
        .getByRole("button", { name: "Single Player", exact: true })
        .click();
    await page.evaluate((resumed) => {
      const start = Reflect.get(window, "aoeStart");
      const nav = Reflect.get(window, "aoeNavigation");
      const scene = document.getElementById("scene");
      const connection = document.getElementById("connection");
      if (!scene || !connection) throw new Error("scene UI missing");
      scene.dataset["frame"] = "1";
      scene.dataset["chunksPending"] = scene.dataset["chunksFailed"] = "0";
      connection.textContent = "connected";
      if (resumed)
        start.resume({ phase: "ready", content_hash: "b".repeat(64) });
      else {
        start.begin();
        start.activated({
          start_available: true,
          content_hash: "b".repeat(64),
        });
      }
      // The renderer publishes markers after applying input and drawing.
      // frame() only consumes navigation input since the canvas sync split.
      nav.present(
        new Float64Array([256, 256, 64, 64, 64, 64, 1200, 800, 1, 0, 1]),
      );
      void nav.map(new Uint8Array(32).fill(187));
      scene.dataset["frame"] = "2";
    }, resumed);
    await expect.poll(() => requests).toBe(1);
    await page.clock.fastForward(45_001);
    await expect(page.locator("#minimap")).toHaveAttribute(
      "data-raster",
      "failed",
    );
    await expect(page.locator("#loading-screen")).toBeVisible();
    await page.clock.fastForward(5000);
    await expect.poll(() => requests).toBe(2);
    await expect(page.locator("#minimap")).toHaveAttribute(
      "data-raster",
      "ready",
    );
    await page.clock.fastForward(1000);
    await expect(page.locator("#start-sequence")).toBeHidden();
    await expect(page.locator("body")).toHaveAttribute(
      "data-start-state",
      "game",
    );
  });
}
