import { expect } from "@playwright/test";
import { test } from "../../server.js";

test("first far zoom reveals kingdom commands in steps, skips immediately and stays dismissed for the match", async ({
  page,
}) => {
  await page.route("**/reveal-test", (route) =>
    route.fulfill({
      contentType: "text/html",
      body: '<link rel="stylesheet" href="/interface/game.css"><canvas id="scene" style="width:1280px;height:720px"></canvas>',
    }),
  );
  await page.goto("/reveal-test");
  await page.evaluate(async () => {
    localStorage.setItem("aoeworld.resume-token", "fixture-match-one");
    sessionStorage.removeItem("aoeworld.cockpit-reveal");
    // @ts-expect-error Served browser module.
    const { initStrategic } = await import("/navigation/strategic.js");
    const s = initStrategic(
      { disabled: true, canvas: document.createElement("canvas"), revision: 0 },
      { records: () => [], revision: () => 0 },
    );
    (window as unknown as { revealTest: unknown }).revealTest = s;
    s.frame([256, 256, 128, 128, 1280, 720, 0.5, 0]);
  });
  const tour = page.locator("#cockpit-reveal");
  await expect(tour).toBeHidden();
  const far = () =>
    page.evaluate(() =>
      (
        window as unknown as { revealTest: { frame(v: number[]): void } }
      ).revealTest.frame([256, 256, 128, 128, 1280, 720, 0.04, 1]),
    );
  await far();
  await expect(tour).toContainText("Armies & kingdoms");
  await expect(tour).toContainText("General");
  await tour.getByRole("button", { name: "Next", exact: true }).click();
  await expect(tour).toContainText("Units here & building");
  await tour.getByRole("button", { name: "Next", exact: true }).click();
  await expect(tour).toContainText("Jump keys");
  await expect(tour).toContainText("Ctrl+F1");
  await tour.getByRole("button", { name: "Skip tour", exact: true }).click();
  await expect(tour).toBeHidden();
  await page.evaluate(() =>
    (
      window as unknown as { revealTest: { present(): void; reset(): void } }
    ).revealTest.present(),
  );
  await far();
  await expect(tour).toBeHidden();
  // A reconnect clears strategic reports but must not replay this match's tour.
  await page.evaluate(() =>
    (window as unknown as { revealTest: { reset(): void } }).revealTest.reset(),
  );
  await far();
  await expect(tour).toBeHidden();
  // A new match has a new resume token, even on the same map.
  await page.evaluate(() => {
    localStorage.setItem("aoeworld.resume-token", "fixture-match-two");
    (window as unknown as { revealTest: { reset(): void } }).revealTest.reset();
  });
  await far();
  await expect(tour).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(tour).toBeHidden();
});
