import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { activateSyntheticMap } from "../synthetic-map.js";
import { forceCanvas, forceWebGl } from "./backends.js";
import { overviewTerrain, sampleFrames, zoom } from "./fixtures/bigview.js";
import { populationFixture } from "./fixtures/population.js";

test("a large client population stays clustered on the full map and keeps explored fog", async ({
  page,
}, info) => {
  test.setTimeout(120_000);
  if (info.project.name === "canvas") await forceCanvas(page);
  else if (info.project.name !== "webgpu") await forceWebGl(page);
  await gameAssets(page);
  const population = await populationFixture(page);
  await page.goto("/?play=1&perf=1#x=8192.5&y=8192.5&zoom=.25");
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
  await overviewTerrain(page);
  const measurements: Record<string, unknown> = {
    population,
    residentTrees: 81 * 1024,
    sprites: await sampleFrames(page),
  };
  await zoom(page, 700, "markers");
  measurements.markers = await sampleFrames(page);
  await zoom(page, 10000, "map");
  measurements.map = await sampleFrames(page);
  const map = page.locator("#strategic-scene");
  expect(Number(await map.getAttribute("data-clusters"))).toBeLessThan(1000);
  const counts = await map.getAttribute("data-picks");
  const clusters = JSON.parse(counts ?? "[]") as { count: number }[];
  expect(clusters.reduce((sum, cluster) => sum + cluster.count, 0)).toBe(
    population,
  );
  console.log(
    `BIGVIEW MAX ${info.project.name}: ${JSON.stringify(measurements)}`,
  );
  await info.attach("maximum-population-frame-times", {
    body: JSON.stringify(measurements, null, 2),
    contentType: "application/json",
  });
  // At map scale only selected actors need per-actor command-panel details.
  // The complete admitted population must remain in the clusters above.
  const hudActors = JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as unknown[];
  expect(hudActors).toHaveLength(0);
  const mutations = await page.evaluate(async () => {
    const svg = document.getElementById("minimap-map");
    if (!svg) throw new Error("Minimap missing");
    let count = 0;
    const observer = new MutationObserver((records) => {
      count += records.length;
    });
    observer.observe(svg, { attributes: true, childList: true, subtree: true });
    for (let at = 0; at < 20; at++) await new Promise(requestAnimationFrame);
    observer.disconnect();
    return count;
  });
  expect(
    mutations,
    "idle minimap does no DOM work for 16k stationary actors",
  ).toBe(0);
  // Drive the production fog raster with controlled overview cells. Owned
  // markers remain visible; the probe avoids their labels and rectangles.
  const probe = await map.evaluate((element) => {
    const canvas = element as HTMLCanvasElement;
    const context = canvas.getContext("2d");
    if (!context) throw new Error("Strategic canvas missing context");
    const y = canvas.height / 2 + 60;
    for (let x = canvas.width / 2 - 30; x < canvas.width / 2 + 30; x++) {
      const rgb = context.getImageData(x, y, 1, 1).data;
      if (rgb[0] === 124 && rgb[1] === 164 && rgb[2] === 82) return { x, y };
    }
    throw new Error("No unobscured terrain probe found");
  });
  const pixel = () =>
    map.evaluate((element, point) => {
      const context = (element as HTMLCanvasElement).getContext("2d");
      if (!context) throw new Error("Strategic canvas missing context");
      return Array.from(
        context.getImageData(point.x, point.y, 1, 1).data,
      ).slice(0, 3);
    }, probe);
  const fog = (state: number) =>
    page.evaluate(
      ({ value, point }) => {
        const navigation = (
          window as Window & {
            aoeNavigation?: { fog(disabled: boolean, cells: number[]): void };
          }
        ).aoeNavigation;
        const panel = document.getElementById("minimap");
        const canvas = document.getElementById(
          "strategic-scene",
        ) as HTMLCanvasElement;
        if (!navigation || !panel) throw new Error("Navigation missing");
        const z = Number(panel.dataset.zoom),
          cx = Number(panel.dataset.cameraX),
          cy = Number(panel.dataset.cameraY);
        const dx = point.x + 0.5 - canvas.width / 2,
          dy = point.y + 0.5 - canvas.height / 2;
        const x = cx + dx / (128 * z) + dy / (64 * z);
        const y = cy - dx / (128 * z) + dy / (64 * z);
        const index =
          Math.floor((y / Number(panel.dataset.worldHeight)) * 256) * 256 +
          Math.floor((x / Number(panel.dataset.worldWidth)) * 256);
        navigation.fog(
          value === 3,
          value === 1 || value === 2 ? [index | (value << 16)] : [],
        );
        return canvas.width;
      },
      { value: state, point: probe },
    );
  await fog(0);
  await expect.poll(pixel).toEqual([0, 0, 0]);
  await fog(1);
  await expect.poll(pixel).toEqual([56, 74, 37]);
  await fog(2);
  await expect.poll(pixel).toEqual([124, 164, 82]);
  await fog(3);
});
