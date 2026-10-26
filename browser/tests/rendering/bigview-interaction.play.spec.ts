import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { activateSyntheticMap } from "../synthetic-map.js";
import { forceCanvas, forceWebGl } from "./backends.js";
import { ownersFixture } from "./fixtures/owners.js";

import { zoom } from "./fixtures/bigview.js";
import { freshHud } from "../village/fixtures/resources.js";

test("map clusters zoom into their location and selected units keep right-click orders", async ({
  page,
}, info) => {
  test.setTimeout(90_000);
  if (info.project.name === "canvas") await forceCanvas(page);
  else if (info.project.name !== "webgpu") await forceWebGl(page);
  await gameAssets(page);
  const orders = await ownersFixture(page);
  await page.goto("/?play=1#x=8192.5&y=8192.5&zoom=1");
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    info.project.name === "webgpu"
      ? "webgpu"
      : info.project.name === "canvas"
        ? "canvas2d"
        : "webgl2",
  );
  await activateSyntheticMap(page, () => false);
  // Surface activation recentres the camera; wait for the HUD's actor refresh.
  await freshHud(page);
  const actors = JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as { id: number; x: number; y: number }[];
  const scout = actors.find((a) => a.id === 0);
  if (!scout) throw new Error("Primary scout missing");
  await page.mouse.click(scout.x, scout.y - 10);
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "1");
  await zoom(page, 3000, "map");
  await page.mouse.click(900, 420, { button: "right" });
  await expect.poll(() => orders.includes(2)).toBe(true);
  // Cluster picks use the same screen bins that were drawn.
  const clusters = JSON.parse(
    (await page.locator("#strategic-scene").getAttribute("data-picks")) ?? "[]",
  ) as { x: number; y: number; count: number }[];
  const cluster = clusters.find((c) => c.count > 1);
  if (!cluster) throw new Error("Counted cluster missing");
  await page.mouse.click(cluster.x, cluster.y);
  await expect(page.locator("#scene")).toHaveAttribute("data-lod", "markers");
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "1");
  const marker = JSON.parse(
    (await page.locator("#strategic-scene").getAttribute("data-picks")) ?? "[]",
  ) as { x: number; y: number }[];
  if (!marker[0]) throw new Error("Marker missing");
  await page.mouse.click(marker[0].x, marker[0].y);
  await expect(page.locator("#scene")).toHaveAttribute("data-lod", "sprites");
});

test("right-click in map view moves a selected unit through the real server", async ({
  page,
  request,
}, info) => {
  test.setTimeout(60_000);
  expect((await request.post("/maps/reset")).status()).toBe(204);
  if (info.project.name === "canvas") await forceCanvas(page);
  else if (info.project.name !== "webgpu") await forceWebGl(page);
  await gameAssets(page);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    info.project.name === "webgpu"
      ? "webgpu"
      : info.project.name === "canvas"
        ? "canvas2d"
        : "webgl2",
  );
  await expect(page.locator("#minimap-entities rect").first()).toBeAttached();
  await page.getByRole("button", { name: "Center on primary unit" }).click();
  await freshHud(page);
  const actors = JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as { kind: string; x: number; y: number }[];
  const scout = actors.find((actor) => actor.kind === "Scout");
  if (!scout) throw new Error("Primary scout missing");
  await page.mouse.click(scout.x, scout.y - 10);
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "1");
  await activateSyntheticMap(page, () => false);
  await zoom(page, 3000, "map");
  await page.mouse.click(700, 360, { button: "right" });
  await expect(page.locator("#unit-state")).toHaveText("unit moving");
});
