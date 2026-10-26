import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { activateSyntheticMap } from "../synthetic-map.js";
import { forceCanvas, forceWebGl } from "./backends.js";
import { ownersFixture } from "./fixtures/owners.js";

import { overviewTerrain, sampleFrames, zoom } from "./fixtures/bigview.js";

test("zoom traverses sprite, marker and map LODs with bounded cached terrain on both GPU tiers", async ({
  page,
}, info) => {
  test.setTimeout(120_000);
  if (info.project.name === "canvas") await forceCanvas(page);
  else if (info.project.name !== "webgpu") await forceWebGl(page);
  await gameAssets(page);
  await ownersFixture(page);
  await page.goto("/?play=1&perf=1#x=8192.5&y=8192.5&zoom=0.25");
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    info.project.name === "webgpu"
      ? "webgpu"
      : info.project.name === "canvas"
        ? "canvas2d"
        : "webgl2",
  );
  await activateSyntheticMap(page, true);
  const terrainRequests = await overviewTerrain(page);
  await expect(page.locator("#scene")).toHaveAttribute("data-lod", "sprites");
  const measurements: Record<string, unknown> = {
    sprites: await sampleFrames(page),
  };
  await zoom(page, 700, "markers");
  await expect(page.locator("#strategic-scene")).toBeVisible();
  measurements.markers = await sampleFrames(page);
  await zoom(page, 1100, "map");
  measurements.map = await sampleFrames(page);
  const redraws = await page
    .locator("#strategic-scene")
    .getAttribute("data-redraws");
  await sampleFrames(page);
  await expect(page.locator("#strategic-scene")).toHaveAttribute(
    "data-redraws",
    redraws ?? "",
  );
  const centre = await page.locator("#strategic-scene").evaluate((canvas) => {
    const c = canvas as HTMLCanvasElement;
    const context = c.getContext("2d");
    if (!context) throw new Error("Strategic canvas missing context");
    return Array.from(context.getImageData(200, 200, 1, 1).data);
  });
  expect(centre.slice(0, 3)).toEqual([124, 164, 82]);
  expect(terrainRequests()).toBe(1);
  await expect(page.locator("#strategic-scene")).toHaveAttribute(
    "data-clusters",
    /^[1-9]\d*$/,
  );
  await expect(page.locator("#strategic-scene")).toHaveAttribute(
    "data-terrain-draws",
    "1",
  );
  const terrain = await page.locator("#minimap-terrain").getAttribute("href");
  await zoom(page, 10000, "map");
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-zoom",
    String(1 / 4096),
  );
  expect(await page.locator("#minimap-terrain").getAttribute("href")).toBe(
    terrain,
  );
  await zoom(page, -3600, "markers");
  await zoom(page, -2000, "sprites");
  await expect(page.locator("#strategic-scene")).toBeHidden();
  await expect(page.locator("#performance-overlay")).toContainText("Frame:");
  console.log(`BIGVIEW ${info.project.name}: ${JSON.stringify(measurements)}`);
  await info.attach("lod-frame-times", {
    body: JSON.stringify(measurements, null, 2),
    contentType: "application/json",
  });
});
