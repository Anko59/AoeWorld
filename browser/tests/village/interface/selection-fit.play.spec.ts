import { expect } from "@playwright/test";
import { isolatedTest as test } from "../../../server.js";
import { nativeMenu, zoomFor } from "../fixtures/native-menu.js";

// The selection panel must never need scrolling and never hide content: every
// state that can overflow is injected into the real panel (real CSS, real
// modules, synthetic frames) and measured. Owner rule: no scrolling in the
// bottom menu, and clipping is as bad as scrolling.
const states = [
  ["5 villagers", "multi", 5, "villagers"],
  ["40 mixed units", "multi", 40],
  ["200 mixed units", "multi", 200],
  ["1000 units of 12 kinds", "multi", 1000, "many"],
  ["one unit", "unit"],
  ["General roster, 10 squads", "general", "captain", 10],
  ["General trainer sliders", "general", "trainer", 10],
  ["Steward sliders", "steward"],
  ["Town Center, queue 15, garrison 15", "tc", 15, 15],
  ["Castle, queue 15, garrison 20, Vassal", "castle"],
  ["Market", "market"],
  ["foundation", "foundation"],
] as const;

type Measure = {
  scroll: string[];
  clip: string[];
  icons: { layout: string; tokens: number; minTok: number } | null;
};

test("Town Center queued villagers and garrison portraits remain visible and clickable", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await nativeMenu(page);
  await page.addScriptTag({
    path: new URL("../fixtures/selection-states.js", import.meta.url).pathname,
  });
  await page.waitForFunction(() => "__h" in window);
  await page.evaluate(async () => {
    await Reflect.get(window, "__h").tc(15, 15, true);
  });
  const queue = page.locator("#training-queue .unit-token");
  await expect(queue).toHaveCount(15);
  for (const token of await queue.all()) {
    await expect(token).toBeVisible();
    const box = await token.boundingBox();
    if (!box) throw new Error("Queued villager is missing");
    expect(box.height).toBeGreaterThanOrEqual(22);
  }
  const button = page.locator("#garrison-units button").last();
  const portrait = button.locator(".art-icon");
  const [target, art, container] = await Promise.all([
    button.boundingBox(),
    portrait.boundingBox(),
    page.locator("#garrison-units").boundingBox(),
  ]);
  if (!target || !art || !container)
    throw new Error("Garrison portrait is missing");
  expect(target.width).toBeGreaterThanOrEqual(28);
  expect(target.height).toBeGreaterThanOrEqual(28);
  expect(art.x).toBeGreaterThanOrEqual(target.x);
  expect(art.y).toBeGreaterThanOrEqual(target.y);
  expect(art.x + art.width).toBeLessThanOrEqual(target.x + target.width + 0.1);
  expect(art.y + art.height).toBeLessThanOrEqual(
    target.y + target.height + 0.1,
  );
  expect(target.y + target.height).toBeLessThanOrEqual(
    container.y + container.height + 0.1,
  );
  await button.click({ trial: true });
});

for (const [width, height] of [
  [1920, 1080],
  [1366, 768],
  [1200, 800],
  [1201, 800],
] as const) {
  test(`the selection panel never scrolls or clips at ${width}x${height}`, async ({
    page,
  }) => {
    test.setTimeout(120_000);
    await page.setViewportSize({ width, height });
    await nativeMenu(page);
    await page.addScriptTag({
      path: new URL("../fixtures/selection-states.js", import.meta.url)
        .pathname,
    });
    await page.waitForFunction(() => "__h" in window);
    const zoom = zoomFor(width);
    for (const [name, fn, ...args] of states) {
      await page.evaluate(
        ([f, a]) =>
          (
            window as unknown as {
              __h: Record<string, (...x: unknown[]) => Promise<void>>;
            }
          ).__h[f as string]?.(...(a as unknown[])),
        [fn, args] as const,
      );
      const m = await page.evaluate(() =>
        (
          window as unknown as { __h: { measure: () => Measure } }
        ).__h.measure(),
      );
      expect(
        m.scroll,
        `${name}: scrolls ${m.scroll.join(" | ")} ${JSON.stringify(m.icons)}`,
      ).toEqual([]);
      expect(m.clip, `${name}: clips ${m.clip.join(" | ")}`).toEqual([]);
      if (m.icons) {
        // Every icon stays a usable click target, even when identical units are grouped.
        expect(m.icons.minTok, `${name}: icon size`).toBeGreaterThanOrEqual(
          27 * zoom,
        );
        if (name.startsWith("200") || name.startsWith("1000"))
          expect(m.icons.layout, `${name}: grouped`).toBe("groups");
      }
      await page.locator("#bottom-panel").screenshot({
        path: `../.cache/nosroll/synthetic-${width}-${name.replace(/\W+/g, "-")}.png`,
      });
    }
  });
}
