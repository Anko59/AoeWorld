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
