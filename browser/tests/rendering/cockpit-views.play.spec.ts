import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { forceWebGl } from "./backends.js";

test("saved views recall position and zoom without changing map layers", async ({
  page,
  request,
}) => {
  test.setTimeout(60_000);
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await forceWebGl(page);
  await gameAssets(page);
  await page.goto("/?play=1#x=110&y=128&zoom=.04");
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    "webgl2",
    { timeout: 30_000 },
  );
  await expect(page.locator("#connection")).toHaveText("connected");
  const camera = page.locator("#minimap");
  await expect(camera).toHaveAttribute("data-camera-x", "110");
  await page.keyboard.press("Control+F1");
  await page.evaluate(() => {
    location.hash = "x=180&y=160&zoom=.5";
  });
  await expect(camera).toHaveAttribute("data-zoom", "0.5");
  await page.keyboard.press("Shift+F1");
  await expect(camera).toHaveAttribute("data-camera-x", "110");
  await expect(camera).toHaveAttribute("data-camera-y", "128");
  await expect(camera).toHaveAttribute("data-zoom", "0.04");
  await expect(
    page.getByRole("button", { name: "Ownership F1", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  await page.getByRole("button", { name: "Main base H", exact: true }).click();
  await expect(page.locator("#selection")).toHaveAttribute(
    "data-kind",
    "TownCenter",
  );
  await expect(camera).toHaveAttribute("data-zoom", "0.04");
  // H moves the camera without changing the URL. Recalling an unchanged
  // fragment must still restore the saved camera, including its zoom.
  await page.keyboard.press("Shift+F1");
  await expect(camera).toHaveAttribute("data-camera-x", "110");
  await expect(camera).toHaveAttribute("data-camera-y", "128");
});

test("view targets use the largest own army and newest real alert; editable keys and reset are safe", async ({
  page,
}) => {
  await page.route("**/views-test", (route) =>
    route.fulfill({
      contentType: "text/html",
      body: '<canvas id="scene"></canvas><input id="typing">',
    }),
  );
  await page.goto("/views-test");
  await page.evaluate(async () => {
    // @ts-expect-error Served browser module.
    const { initViews } = await import("/navigation/views.js");
    const jumps: number[][] = [];
    const views = initViews((point: number[]) => jumps.push(point));
    const own = (id: number, x: number, size: number, owner = 1) => [
      id,
      x,
      100,
      owner,
      0,
      size,
      0,
      0,
      NaN,
      NaN,
      0,
      0,
      0,
      0,
      0,
    ];
    views.present([256, 256, 128, 128, 0, 0, 1280, 720, 0.04]);
    views.show(true);
    views.armies([...own(2, 30, 10), ...own(1, 40, 20), ...own(3, 90, 100, 0)]);
    views.alert(5, 70, 80);
    views.alert(4, 10, 20);
    (window as unknown as { viewsTest: unknown }).viewsTest = { views, jumps };
  });
  await page.keyboard.press("End");
  await page.keyboard.press("Space");
  expect(
    await page.evaluate(
      () =>
        (window as unknown as { viewsTest: { jumps: number[][] } }).viewsTest
          .jumps,
    ),
  ).toEqual([
    [40, 100, 0.04],
    [70, 80, 0.04],
  ]);
  await page.locator("#typing").focus();
  await page.keyboard.press("Control+F2");
  await expect(
    page.getByRole("button", { name: "View 2", exact: true }),
  ).toBeDisabled();
  await page.evaluate(() =>
    (
      window as unknown as { viewsTest: { views: { reset(): void } } }
    ).viewsTest.views.reset(),
  );
  await expect(
    page.getByRole("button", { name: "Largest army End", exact: true }),
  ).toBeDisabled();
  await expect(
    page.getByRole("button", { name: "Last alert Space", exact: true }),
  ).toBeDisabled();
});
