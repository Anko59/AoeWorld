import { expect } from "@playwright/test";
import { test } from "../../server.js";

test("close view projects separate boundary segments and clears removed zones", async ({
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
      [1, 1, 1, 2, 10, 10, 80, 10, 40, 80, 40, 130],
      "",
      [],
    );
  });
  const outline = page.locator('#steward-domain path[data-office="vassal"]');
  await expect(outline).toBeVisible();
  await expect(outline).toHaveAttribute("d", "M10,10L80,10M40,80L40,130");
  await expect(outline).toHaveAttribute("data-selected", "true");
  await page.evaluate(() =>
    Reflect.get(window, "aoeSteward").update([], "", []),
  );
  await expect(outline).toBeHidden();
});

test("far view draws the same open segments for both office types", async ({
  page,
}) => {
  await page.route("**/domain-test", (route) =>
    route.fulfill({
      contentType: "text/html",
      body: '<canvas id="scene"></canvas>',
    }),
  );
  await page.goto("/domain-test");
  const pixels = await page.evaluate(async () => {
    // @ts-expect-error Served browser module.
    const { initTokens } = await import("/navigation/tokens.js");
    const tokens = initTokens();
    const context = document.querySelector("canvas")?.getContext("2d");
    if (!context) throw new Error("Canvas context is missing");
    const plane = () => context.setTransform(2, 0, 0, 3, 0, 0);
    const samples = [
      [7, 12],
      [20, 39],
      [16, 24],
      [14, 30],
    ] as const;
    const alphas = () =>
      samples.map(
        ([x, y]) => (context.getImageData(x, y, 1, 1).data[3] ?? 0) > 0,
      );
    tokens.zone(10, false, [2, 4, 5, 4, 10, 12, 10, 14]);
    tokens.zone(20, true, [7, 8, 9, 8]);
    tokens.drawZones(context, { ownership: true }, plane, 1 / 64);
    const before = alphas();
    context.clearRect(0, 0, 300, 150);
    tokens.zone(10, false, []);
    tokens.drawZones(context, { ownership: true }, plane, 1 / 64);
    return [before, alphas()];
  });
  expect(pixels).toEqual([
    [true, true, true, false],
    [false, false, true, false],
  ]);
});
