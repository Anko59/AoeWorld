import { expect, type Page } from "@playwright/test";
import { test } from "../../server.js";

type RevealWindow = Window & {
  revealTest: {
    frame(v: number[]): void;
    navigation(v: number[]): number[] | null;
    reset(): void;
  };
};
async function setup(page: Page) {
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
    (window as unknown as RevealWindow).revealTest = s;
    s.navigation([8000, 8000, 500, 500, 500, 500, 1280, 720, 0.5, 16, 1]);
    s.frame([8000, 8000, 500, 500, 1280, 720, 0.5, 0]);
  });
}
const far = (page: Page) =>
  page.evaluate(() =>
    (window as unknown as RevealWindow).revealTest.frame([
      8000, 8000, 500, 500, 1280, 720, 0.04, 1,
    ]),
  );
const camera = (page: Page) =>
  page.evaluate(() =>
    (window as unknown as RevealWindow).revealTest.navigation([
      8000, 8000, 500, 500, 500, 500, 1280, 720, 0.04, 16, 1,
    ]),
  );

test("first zoom pulls back, reveals the land, then brings in the cockpit once per match", async ({
  page,
}) => {
  await setup(page);
  await page.clock.install();
  await far(page);
  const reveal = page.locator("#cockpit-reveal");
  await expect(reveal).toBeVisible();
  await expect(page.locator("html")).toHaveAttribute(
    "data-cockpit-reveal",
    "pullback",
  );
  const initial = await camera(page);
  expect(initial?.[2]).toBeCloseTo(0.5, 1);
  await page.clock.runFor(1500);
  const middle = await camera(page);
  expect(middle?.[2]).toBeLessThan(initial?.[2] ?? 0);
  expect(middle?.[2]).toBeGreaterThan(0.001);
  await page.clock.runFor(1800);
  await camera(page);
  await expect(page.locator("html")).toHaveAttribute(
    "data-cockpit-reveal",
    "land",
  );
  await expect(reveal).toContainText("A kingdom worth ruling");
  await page.clock.runFor(1200);
  await camera(page);
  await expect(page.locator("html")).toHaveAttribute(
    "data-cockpit-reveal",
    "cockpit",
  );
  await page.clock.runFor(1500);
  await camera(page);
  await expect(reveal).toBeHidden();
  await page.evaluate(() =>
    (window as unknown as RevealWindow).revealTest.reset(),
  );
  await far(page);
  await expect(reveal).toBeHidden();
  await page.evaluate(() => {
    localStorage.setItem("aoeworld.resume-token", "fixture-match-two");
    (window as unknown as RevealWindow).revealTest.reset();
  });
  await far(page);
  await expect(reveal).toBeVisible();
  await reveal.getByRole("button", { name: "Skip", exact: true }).click();
  await expect(reveal).toBeHidden();
  await expect(page.locator("html")).not.toHaveAttribute(
    "data-cockpit-reveal",
    /.+/,
  );
});

test("reveal skips from the pull-back and respects reduced motion", async ({
  page,
}) => {
  await setup(page);
  await far(page);
  await expect(page.locator("#cockpit-reveal")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.locator("#cockpit-reveal")).toBeHidden();
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.evaluate(() => {
    localStorage.setItem("aoeworld.resume-token", "reduced-motion-match");
    const s = (window as unknown as RevealWindow).revealTest;
    s.reset();
    s.navigation([8000, 8000, 500, 500, 500, 500, 1280, 720, 0.5, 16, 1]);
  });
  expect(await camera(page)).toBeNull();
  await far(page);
  await expect(page.locator("#cockpit-reveal")).toBeVisible();
  expect(await camera(page)).toBeNull();
  await expect(page.locator("html")).toHaveAttribute(
    "data-cockpit-reveal",
    "cockpit",
  );
  await page.mouse.wheel(0, 200);
  await expect(page.locator("#cockpit-reveal")).toBeHidden();
});

test("Skip during pull-back settles once and a camera link interrupts the moment", async ({
  page,
}) => {
  await setup(page);
  await far(page);
  await expect(page.locator("#cockpit-reveal")).toBeVisible();
  await page
    .locator("#cockpit-reveal")
    .getByRole("button", { name: "Skip", exact: true })
    .click();
  const final = await camera(page);
  expect(final?.[2]).toBeLessThan(0.002);
  expect(await camera(page)).toBeNull();
  await page.evaluate(() => {
    localStorage.setItem("aoeworld.resume-token", "camera-link-match");
    const s = (window as unknown as RevealWindow).revealTest;
    s.reset();
    s.navigation([8000, 8000, 500, 500, 500, 500, 1280, 720, 0.5, 16, 1]);
  });
  await far(page);
  await expect(page.locator("#cockpit-reveal")).toBeVisible();
  await page.evaluate(() => {
    location.hash = "x=500&y=500&zoom=.5";
  });
  await expect(page.locator("#cockpit-reveal")).toBeHidden();
  expect(await camera(page)).toBeNull();
});

test("pull-back reuses the village surface and a layer key restores the requested far camera", async ({
  page,
}) => {
  await setup(page);
  await far(page);
  await expect(page.locator("#cockpit-reveal")).toBeVisible();
  await camera(page);
  const state = await page.evaluate(() => {
    const s = (window as unknown as { revealTest: { cinematic(): boolean } })
      .revealTest;
    return {
      active: s.cinematic(),
      scale: document.documentElement.style.getPropertyValue("--reveal-scale"),
    };
  });
  expect(state.active).toBe(true);
  expect(Number(state.scale)).toBeGreaterThan(0);
  await page.keyboard.press("F3");
  const restored = await camera(page);
  expect(restored?.[2]).toBe(0.04);
  expect(await camera(page)).toBeNull();
  await expect(page.locator("#cockpit-reveal")).toBeHidden();
});

test("the pull-back keeps the input canvas full size and delivers the first ground click", async ({
  page,
}) => {
  await setup(page);
  await page.evaluate(() => {
    const scene = document.getElementById("scene");
    if (!scene) throw new Error("Input canvas missing");
    scene.addEventListener("pointerdown", (event) =>
      scene.setAttribute("data-first-button", String(event.button)),
    );
  });
  await far(page);
  await expect(page.locator("#cockpit-reveal")).toBeVisible();
  await page.clock.install();
  await page.clock.runFor(1200);
  await camera(page);
  const scene = page.locator("#scene");
  await expect(scene).toHaveCSS("transform", "none");
  expect(await scene.boundingBox()).toEqual({
    x: 0,
    y: 0,
    width: 1280,
    height: 720,
  });
  await page.mouse.click(900, 420, { button: "right" });
  await expect(scene).toHaveAttribute("data-first-button", "2");
  await expect(page.locator("#cockpit-reveal")).toBeHidden();
});
