import { access, readFile } from "node:fs/promises";
import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { forceWebGl } from "./backends.js";

test("real server cockpit counts own soldiers and follows authoritative stock and population", async ({
  page,
  request,
}) => {
  test.setTimeout(60_000);
  expect(
    (await request.post("/maps/reset?scenario=general-army")).status(),
  ).toBe(204);
  await forceWebGl(page);
  await gameAssets(page);
  await page.goto("/?play=1#x=110&y=128&zoom=.04");
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    "webgl2",
    { timeout: 30_000 },
  );
  await expect(page.locator("#connection")).toHaveText("connected");
  const panel = page.locator("#strategic-cockpit");
  await expect(panel).toBeVisible();
  // The scenario also contains five enemy Militia. They must never enter this census.
  await expect(panel).toContainText("Militia: 4");
  await expect(panel).toContainText("Archer: 4");
  await expect(panel).toContainText("Knight: 2");
  await expect(panel.locator('[data-stock="food"]')).toHaveText(
    Number(await page.locator("#stockpile-food").innerText()).toLocaleString(
      "en-US",
    ),
  );
  await expect(panel.locator("[data-population]")).toHaveText(
    (await page.locator("#population").innerText()).replace("/", " / "),
  );
  await page.evaluate(() => {
    location.hash = "x=110&y=128&zoom=.5";
  });
  await expect(panel).toBeHidden();
});

test("cockpit reports real deliveries and owned army types, and clears on reset", async ({
  page,
}) => {
  await page.route("**/cockpit-test", (route) =>
    route.fulfill({
      contentType: "text/html",
      body: '<link rel="stylesheet" href="/interface/game.css"><canvas id="scene" style="width:1280px;height:720px"></canvas>',
    }),
  );
  await page.goto("/cockpit-test");
  await page.evaluate(async () => {
    // @ts-expect-error Served browser module.
    const { initStrategic } = await import("/navigation/strategic.js");
    const strategic = initStrategic(
      { disabled: true, canvas: document.createElement("canvas"), revision: 0 },
      { records: () => [], revision: () => 0 },
    );
    (window as unknown as { cockpitTest: unknown }).cockpitTest = strategic;
    // Stock, population, actual credited deliveries in recent buckets, footprint
    // area, then the fixed authoritative census. No stock-delta estimate.
    // The authoritative census also includes soldiers not in the map stream,
    // such as transport passengers. Map actors must not reconstruct it.
    const census = new Uint32Array(34);
    census[1] = 2;
    census[2] = 5;
    strategic.cockpit([120, 240, 80, 40, 12, 20, 9, 17, 0, 3, 32], census);
    strategic.frame([256, 256, 128, 128, 1280, 720, 0.04, 1]);
  });
  const panel = page.locator("#strategic-cockpit");
  await expect(panel).toBeVisible();
  await expect(panel).toContainText("240");
  await expect(panel).toContainText("17");
  await expect(panel).toContainText("12 / 20");
  await expect(panel).toContainText("32 tiles");
  await expect(panel).toContainText("Scout: 2");
  await expect(panel).toContainText("Militia: 5");
  await expect(panel.locator(".art-icon")).not.toHaveCount(0);
  await page.evaluate(() => {
    const s = (
      window as unknown as { cockpitTest: { reset(): void; present(): void } }
    ).cockpitTest;
    s.reset();
    s.present();
  });
  await expect(panel).toBeHidden();
  await expect(panel).not.toContainText("240");
});

test("local cockpit capture and evidence stay outside the shipped repository", async () => {
  await expect(
    access("tests/rendering/cockpit-capture.play.spec.ts"),
  ).rejects.toThrow();
  await expect(access("../docs/scale/cockpit-evidence.md")).rejects.toThrow();
  expect(await readFile("../docs/strategic-view.md", "utf8")).not.toContain(
    "cockpit-evidence.md",
  );
});
