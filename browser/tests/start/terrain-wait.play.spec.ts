import { expect } from "@playwright/test";
import { test } from "../../server.js";

// Exercise the real start sequence with a stalled terrain adapter and a live
// match. A virtual clock reproduces the old 90-second deadline without load.
test.beforeEach(async ({ page }) => {
  await page.route("**/start/bootstrap.js", (route) =>
    route.fulfill({
      contentType: "application/javascript",
      body: `
        import { initInterface } from '/interface/hud.js';
        import { initStartSequence } from '/start/sequence.js';
        initInterface();
        window.aoeStart = initStartSequence({
          readJson: async response => response.json(), start() {}, cancel() {},
        });
      `,
    }),
  );
  await page.clock.install();
});

for (const resumed of [false, true]) {
  test(`${resumed ? "resumed" : "new"} live match keeps waiting for terrain past 90 seconds`, async ({
    page,
  }) => {
    if (resumed) {
      await page.addInitScript(() => {
        localStorage.setItem("aoeworld.resume-token", "a".repeat(48));
      });
    }
    await page.goto("/?skipIntro=1");
    if (!resumed) {
      await page
        .getByRole("button", { name: "Single Player", exact: true })
        .click();
    }
    await page.evaluate((resumed) => {
      const start = (
        window as unknown as {
          aoeStart: {
            begin(): void;
            activated(value: unknown): void;
            resume(value: unknown): void;
          };
        }
      ).aoeStart;
      const scene = document.getElementById("scene");
      if (!scene) throw new Error("scene missing");
      scene.dataset["frame"] = "1";
      scene.dataset["chunksPending"] = "7";
      scene.dataset["chunksFailed"] = "0";
      const connection = document.getElementById("connection");
      const observer = document.getElementById("observer-label");
      if (!connection || !observer) throw new Error("connection UI missing");
      connection.textContent = "connected";
      const minimap = document.getElementById("minimap");
      if (!minimap) throw new Error("minimap missing");
      minimap.dataset["mapHash"] = "b".repeat(64);
      minimap.dataset["raster"] = "ready";
      observer.hidden = true;
      if (resumed)
        start.resume({ phase: "ready", content_hash: "b".repeat(64) });
      else {
        start.begin();
        start.activated({
          start_available: true,
          content_hash: "b".repeat(64),
        });
      }
    }, resumed);
    await expect(page.locator("#loading-phase")).toContainText(
      "Opening terrain",
    );
    await page.clock.fastForward(91_000);
    await expect(page.locator("#loading-error")).toBeHidden();
    await expect(page.locator("#loading-screen")).toBeVisible();
    await expect(page.locator("#loading-phase")).toHaveText(
      "Opening terrain · 7 chunks remaining",
    );
    await page.locator("#scene").evaluate((scene) => {
      scene.dataset["chunksPending"] = "2";
    });
    await expect(page.locator("#loading-phase")).toHaveText(
      "Opening terrain · 2 chunks remaining",
    );
    await page.evaluate(() => {
      const scene = document.getElementById("scene");
      if (!scene) throw new Error("scene missing");
      scene.dataset["chunksPending"] = "0";
      scene.dataset["frame"] = "2";
      const minimap = document.getElementById("minimap");
      if (!minimap) throw new Error("minimap missing");
      minimap.dataset["worldWidth"] = minimap.dataset["worldHeight"] = "256";
      minimap.dataset["cameraX"] = minimap.dataset["cameraY"] = "64";
      const primary = document.getElementById("minimap-primary");
      if (!primary) throw new Error("primary missing");
      primary.setAttribute("cx", "64");
      primary.setAttribute("cy", "64");
      primary.setAttribute("visibility", "visible");
    });
    await page.clock.fastForward(1000);
    await expect(page.locator("#start-sequence")).toBeHidden();
    await expect(page.locator("body")).toHaveAttribute(
      "data-start-state",
      "game",
    );
  });
}
