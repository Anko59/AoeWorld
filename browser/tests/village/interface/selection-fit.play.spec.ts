import { expect } from "@playwright/test";
import { test } from "../../../server.js";
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
      const m = await page.evaluate(
        () =>
          (
            window as unknown as { __h: { measure: () => Measure } }
          ).__h.measure(),
      );
      expect(m.scroll, `${name}: scrolls ${m.scroll.join(" | ")} ${JSON.stringify(m.icons)}`).toEqual([]);
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
