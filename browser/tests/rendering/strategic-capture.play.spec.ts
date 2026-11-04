import { expect } from "@playwright/test";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { fallbackTest as test, forceWebGl } from "./backends.js";
import { sampleFrames } from "./fixtures/bigview.js";
import { strategicLargeProof } from "../proofs/strategic/large.js";

// Local original-art proof. The normal regression spec uses disposable servers;
// this explicitly requested take uses only this checkout's make dev on 8153.
test("local far zoom film", async ({ browser }, info) => {
  test.skip(
    info.project.name !== "browser-defaults",
    "Local WebGL2 take uses the normal compositor",
  );
  const config = await readFile("../.cache/showcase/zoom.json", "utf8").then(
    (text) => JSON.parse(text) as { url: string; mode: string },
    () => null,
  );
  test.skip(!config, "Local capture was not requested");
  if (!config) return;
  expect(config.url).toBe("http://127.0.0.1:8153/");
  test.setTimeout(config.mode === "large" ? 600_000 : 180_000);
  await mkdir("../.cache/showcase", { recursive: true });
  const context = await browser.newContext({
    viewport: { width: 1280, height: 720 },
    recordVideo: {
      dir: "../.cache/showcase/raw",
      size: { width: 1280, height: 720 },
    },
  });
  const page = await context.newPage();
  const started = Date.now();
  await forceWebGl(page);
  try {
    if (config.mode === "large") {
      await strategicLargeProof(page, config.url, started);
      return;
    }
    const scenario =
      config.mode === "steward" ? "steward-kingdom" : "general-army";
    expect(
      (
        await page.request.post(`${config.url}maps/reset?scenario=${scenario}`)
      ).status(),
    ).toBe(204);
    await page.goto(
      `${config.url}?play=1&perf=1#x=106&y=128&zoom=.5&renderer=webgl2`,
    );
    await expect(page.locator("#connection")).toHaveText("connected", {
      timeout: 60_000,
    });
    await expect(page.locator("#playground")).toHaveAttribute(
      "data-renderer",
      "webgl2",
    );
    if (config.mode !== "steward") {
      type Actor = { kind: string; x: number; y: number };
      await expect
        .poll(async () => {
          const actors: Actor[] = JSON.parse(
            (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
          );
          const castle = actors.find((a) => a.kind === "Castle");
          if (!castle) return false;
          await page.mouse.click(castle.x, castle.y);
          return page.locator("#train-general").isVisible();
        })
        .toBe(true);
      await page.locator("#train-general").click();
      await expect
        .poll(
          async () =>
            (
              await page.locator("#scene").getAttribute("data-actors")
            )?.includes('"General"'),
          { timeout: 30_000 },
        )
        .toBe(true);
    }
    await page.evaluate(() => {
      location.hash = "x=128&y=128&zoom=.04";
    });
    await expect(page.locator("#scene")).toHaveAttribute("data-lod", "markers");
    const target = page
      .locator(
        config.mode === "steward"
          ? ".steward-card"
          : ".army-token[data-owned='true']",
      )
      .first();
    await expect(target).toBeVisible();
    await page.waitForTimeout(1000);
    const ready = (Date.now() - started) / 1000;
    await page.screenshot({
      path: `../.cache/showcase/${config.mode}-first.png`,
    });
    await page.waitForTimeout(1500);
    await target.click();
    if (config.mode !== "steward") {
      await expect(page.locator("#selection")).toHaveAttribute(
        "data-kind",
        "General",
      );
      await page.mouse.click(810, 380, { button: "right" });
      await expect(target).toHaveAttribute("data-order", "marching");
    } else {
      await expect(page.locator("#selection")).toHaveAttribute(
        "data-kind",
        "TownCenter",
      );
      await expect(page.locator("#steward-status")).toContainText(
        "domain active",
      );
    }
    await page.waitForTimeout(3500);
    await page.screenshot({
      path: `../.cache/showcase/${config.mode}-middle.png`,
    });
    const layerKey = config.mode === "steward" ? "F3" : "F5";
    await page.keyboard.press(layerKey);
    await page.waitForTimeout(1000);
    await page.screenshot({
      path: `../.cache/showcase/${config.mode}-layer-off.png`,
    });
    await page.keyboard.press(layerKey);
    await page.waitForTimeout(1000);
    await page.screenshot({
      path: `../.cache/showcase/${config.mode}-last.png`,
    });
    const scene = await page
      .locator("#strategic-scene")
      .evaluate((e) => ({ ...e.dataset }));
    const data = {
      ready,
      duration: (Date.now() - started) / 1000 - ready,
      scene,
      camera: await page
        .locator("#minimap")
        .evaluate((e) => ({ ...e.dataset })),
      tokens: await page.locator("#strategic-tokens").innerText(),
      performance: await sampleFrames(page),
    };
    await writeFile(
      `../.cache/showcase/${config.mode}-capture.json`,
      JSON.stringify(data, null, 2),
    );
  } finally {
    await page
      .screenshot({ path: `../.cache/showcase/${config.mode}-debug.png` })
      .catch(() => null);
    await context.close();
    await page.video()?.saveAs(`../.cache/showcase/${config.mode}-raw.webm`);
  }
});

// Opt-in source-backed before/after take. No socket, terrain, actor or art mocks.
test("local cockpit3 source landscape film and frame cost", async ({
  browser,
}, info) => {
  test.skip(
    info.project.name !== "browser-defaults",
    "One headless compositor take",
  );
  const config = await readFile(
    "../.cache/showcase/cockpit3.json",
    "utf8",
  ).then(
    (text) => JSON.parse(text) as { url: string; phase: string },
    () => null,
  );
  test.skip(!config, "Local original-art take was not requested");
  if (!config) return;
  expect(config.url).toBe("http://127.0.0.1:8155/");
  expect(["before", "after"]).toContain(config.phase);
  test.setTimeout(180_000);
  const prefix = `../.cache/showcase/c3-${config.phase}`;
  const context = await browser.newContext({
    viewport: { width: 1280, height: 720 },
    recordVideo: {
      dir: "../.cache/showcase/c3-raw",
      size: { width: 1280, height: 720 },
    },
  });
  const page = await context.newPage();
  const started = Date.now();
  await forceWebGl(page);
  try {
    await expect
      .poll(
        async () => {
          try {
            return (await page.request.get(`${config.url}health`)).ok();
          } catch {
            return false;
          }
        },
        { timeout: 90_000, intervals: [1000] },
      )
      .toBe(true);
    await page.goto(`${config.url}?play=1&perf=1`);
    await expect(page.locator("#connection")).toHaveText("connected", {
      timeout: 90_000,
    });
    await expect(page.locator("#minimap")).toHaveAttribute(
      "data-world-width",
      "8000",
    );
    await expect(page.locator("#minimap")).toHaveAttribute(
      "data-raster",
      "ready",
      { timeout: 90_000 },
    );
    await expect(page.locator("html")).toHaveAttribute(
      "data-interface-art",
      "original",
      { timeout: 60_000 },
    );
    await page.keyboard.press("Home");
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-chunks-pending",
      "0",
      { timeout: 90_000 },
    );
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-chunks-failed",
      "0",
    );
    await page.waitForTimeout(2500);
    const ready = (Date.now() - started) / 1000;
    await page.screenshot({ path: `${prefix}-village.png` });
    await page.mouse.move(640, 330);
    await page.mouse.wheel(0, 1625);
    await page.waitForTimeout(1400);
    await page.screenshot({ path: `${prefix}-reveal.png` });
    await page.waitForTimeout(2200);
    await page.screenshot({ path: `${prefix}-land.png` });
    await page.waitForTimeout(1300);
    await page.screenshot({ path: `${prefix}-cockpit.png` });
    await page.waitForTimeout(3000);
    await page.keyboard.press("Escape");
    await page.screenshot({ path: `${prefix}-kingdom.png` });
    const regional = await sampleFrames(page);
    await page.evaluate(() => {
      location.hash = "x=4000&y=4000&zoom=.001";
    });
    await page.waitForTimeout(1800);
    await page.screenshot({ path: `${prefix}-world.png` });
    // Keep the measured camera on land; an unrestricted far-view arrow pan
    // can leave an 8,000-tile world before the sample window finishes.
    const motion = page.evaluate(async () => {
      let key = "";
      try {
        for (let i = 0; i < 60; i++) {
          if (i % 5 === 0) {
            if (key)
              document.dispatchEvent(
                new KeyboardEvent("keyup", { key, bubbles: true }),
              );
            key = i % 10 === 0 ? "ArrowRight" : "ArrowLeft";
            document.dispatchEvent(
              new KeyboardEvent("keydown", { key, bubbles: true }),
            );
          }
          await new Promise(requestAnimationFrame);
        }
      } finally {
        if (key)
          document.dispatchEvent(
            new KeyboardEvent("keyup", { key, bubbles: true }),
          );
      }
    });
    const moving = await sampleFrames(page);
    await motion;
    const data = {
      ready,
      duration: (Date.now() - started) / 1000 - ready,
      regional,
      moving,
      scene: await page
        .locator("#strategic-scene")
        .evaluate((e) => ({ ...e.dataset })),
      camera: await page
        .locator("#minimap")
        .evaluate((e) => ({ ...e.dataset })),
      health: (await (
        await page.request.get(`${config.url}health`)
      ).json()) as unknown,
    };
    await writeFile(`${prefix}-capture.json`, JSON.stringify(data, null, 2));
  } finally {
    await context.close();
    await page.video()?.saveAs(`${prefix}-raw.webm`);
  }
});
