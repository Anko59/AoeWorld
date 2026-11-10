import { expect } from "@playwright/test";
import { test } from "../../server.js";

test("inactive Town Center domains disappear while active plain outlines remain", async ({
  page,
}) => {
  await page.route("**/domain-test", (route) =>
    route.fulfill({
      contentType: "text/html",
      body: '<div id="selection-content"></div>',
    }),
  );
  await page.goto("/domain-test");
  await page.evaluate(async () => {
    // @ts-expect-error Served browser module.
    const { initSteward } = await import("/interface/steward/panel.js");
    initSteward();
    Reflect.get(window, "aoeSteward").update(
      [2, 0, 0, 3, 10, 10, 80, 10, 40, 80, 1, 1, 3, 100, 10, 180, 10, 140, 80],
      "",
      [],
    );
  });
  await expect(
    page.locator('#steward-domain polygon[data-active="false"]'),
  ).toBeHidden();
  const active = page.locator('#steward-domain polygon[data-active="true"]');
  await expect(active).toBeVisible();
  await expect(active).toHaveAttribute("stroke-dasharray", "");
  // A dismissed Steward must hide the same reused polygon on the next HUD refresh.
  await page.evaluate(() =>
    Reflect.get(window, "aoeSteward").update(
      [1, 0, 1, 3, 100, 10, 180, 10, 140, 80],
      "",
      [],
    ),
  );
  await expect(page.locator("#steward-domain polygon").nth(0)).toBeHidden();
  await expect(page.locator("#steward-domain polygon").nth(1)).toBeHidden();
});

test("far domains draw only the plain active boundary", async ({ page }) => {
  await page.route("**/domain-test", (route) =>
    route.fulfill({
      contentType: "text/html",
      body: '<canvas id="scene"></canvas>',
    }),
  );
  await page.goto("/domain-test");
  const strokes = await page.evaluate(async () => {
    // @ts-expect-error Served browser module.
    const { initTokens } = await import("/navigation/tokens.js");
    const tokens = initTokens();
    const canvas = document.querySelector("canvas");
    const context = canvas?.getContext("2d");
    if (!context) throw new Error("Canvas context is missing");
    const dashes: number[][] = [];
    context.stroke = () => dashes.push(context.getLineDash());
    tokens.update(
      [],
      [
        1, 10, 200, 200, 0, 0, 0, -1, 0, 0, 0, 0, 0, 2, 20, 400, 200, 0, 0, 0,
        0, 0, 0, 0, 0, 0,
      ],
      [],
    );
    tokens.draw(
      context,
      (p: number[]) => p,
      { ownership: true },
      1,
      1280,
      720,
      {},
    );
    return dashes;
  });
  expect(strokes).toEqual([[]]);
});
