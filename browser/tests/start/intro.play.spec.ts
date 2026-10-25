import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";

test.beforeEach(async ({ page, request }) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
});

test("poster mode preserves the four-shot order, closing reveal and duration", async ({
  page,
}) => {
  const epoch = Date.now();
  await page.clock.install({ time: epoch });
  await page.clock.pauseAt(epoch + 1000);
  await page.goto("/?introStills=1");
  const shots = page.locator(".intro-shot");
  await expect(shots).toHaveCount(4);
  const names = ["king", "ship", "cavalry", "castle"];
  for (let index = 0; index < names.length; index++) {
    const video = shots.nth(index).locator("video");
    await expect(video).toHaveAttribute(
      "poster",
      new RegExp(`${names[index]}-poster.jpg$`),
    );
    expect(
      await video.evaluate((node: HTMLVideoElement) => ({
        muted: node.muted,
        inline: node.playsInline,
        loop: node.loop,
        rate: node.playbackRate,
        preload: node.preload,
        source: node.getAttribute("src"),
      })),
    ).toEqual({
      muted: true,
      inline: true,
      loop: false,
      rate: 1,
      preload: "auto",
      source: null,
    });
    await expect(shots.nth(index).locator("img")).toHaveJSProperty(
      "complete",
      true,
    );
  }
  await expect(shots.nth(0)).toHaveClass(/active/);
  for (let index = 1; index < 4; index++) {
    await page.clock.fastForward(4000);
    await expect(shots.nth(index)).toHaveClass(/active/);
  }
  await page.clock.fastForward(3600);
  await expect(page.locator("#entry-intro")).toBeVisible();
  await expect(page.locator("#entry-intro")).toHaveClass(/reveal/);
  await page.clock.fastForward(3000);
  await expect(page.locator("#main-menu")).toBeVisible();
});

test("blocked playback keeps a poster visible and skip stops all media", async ({
  page,
}) => {
  await page.addInitScript(() => {
    HTMLMediaElement.prototype.play = () =>
      Promise.reject(new DOMException("Playback blocked", "NotAllowedError"));
  });
  await page.goto("/");
  await expect(page.locator(".intro-shot")).toHaveCount(4);
  await expect(page.locator(".intro-shot").first()).toHaveAttribute(
    "data-playback",
    "fallback",
  );
  await expect(
    page.locator(".intro-shot").first().locator("img"),
  ).toBeVisible();
  await page.keyboard.press("Space");
  await expect(page.locator("#main-menu")).toBeVisible();
  expect(
    await page
      .locator(".intro-shot video")
      .evaluateAll((nodes) =>
        nodes.every((node) => (node as HTMLVideoElement).paused),
      ),
  ).toBe(true);
  await page.reload();
  await expect(page.locator("#main-menu")).toBeVisible();
  expect(
    await page
      .locator(".intro-shot video")
      .evaluateAll((nodes) => nodes.every((node) => !node.hasAttribute("src"))),
  ).toBe(true);
});

for (const phase of ["idle", "ready"]) {
  test(`fresh tab waits for the resume decision before initializing (${phase})`, async ({
    page,
  }) => {
    await page.addInitScript(() => {
      sessionStorage.clear();
      localStorage.setItem("aoeworld.resume-token", "a".repeat(48));
      Reflect.set(window, "initStates", []);
    });
    await page.route("**/pkg/aoe_client.js", (route) =>
      route.fulfill({
        contentType: "application/javascript",
        body: `export default async function() { window.initStates.push(document.body.dataset.startState); }`,
      }),
    );
    let release!: () => void;
    const decision = new Promise<void>((resolve) => {
      release = resolve;
    });
    await page.route("**/skirmish", async (route) => {
      await decision;
      await route.fulfill({
        json: { phase, content_hash: "b".repeat(64), ai_available: true },
      });
    });
    await page.goto("/?introStills=1");
    await expect(page.locator("body")).toHaveAttribute(
      "data-start-state",
      "loading",
    );
    // A task turn lets the initialization promise run if it was released early.
    await page.evaluate(
      () => new Promise((resolve) => setTimeout(resolve, 100)),
    );
    expect(
      await page.evaluate(() => Reflect.get(window, "initStates")),
    ).toEqual([]);
    release();
    if (phase === "idle") {
      await expect(page.locator("#entry-intro")).toBeVisible();
      expect(
        await page.evaluate(() => Reflect.get(window, "initStates")),
      ).toEqual([]);
      await page.keyboard.press("Space");
      await expect(page.locator("#main-menu")).toBeVisible();
    }
    await expect
      .poll(() => page.evaluate(() => Reflect.get(window, "initStates")))
      .toEqual([phase === "idle" ? "menu" : "loading"]);
  });
}
