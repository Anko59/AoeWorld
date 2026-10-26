import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";

test("minimap pixel aggregation scales dots and resolves owners by count", async ({
  page,
}) => {
  await page.goto("/diagnostics.html");
  const result = await page.evaluate(async () => {
    const { aggregateActors, minimapWorldPoint, minimapProject } = await import(
      "/minimap.js" as string
    );
    const world = [16384, 16384];
    const view = { zoom: 1, center: [8192, 8192] };
    const actors = [
      [0, 8200, 8192, 7, 0xff0000],
      [0, 8201, 8192, 3, 0x0000ff],
      [1, 8200, 8193, 3, 0x0000ff],
    ].flat();
    const bins = aggregateActors(actors, world, view, 140);
    const reversed = aggregateActors(
      [...actors.slice(10), ...actors.slice(5, 10), ...actors.slice(0, 5)],
      world,
      view,
      140,
    );
    const zoomed = { zoom: 8, center: [6000, 7000] };
    const point = [6100, 7100];
    const projected = minimapProject(point, world, zoomed);
    const rect = { left: 10, top: 20, width: 400, height: 300 };
    const back = minimapWorldPoint(
      rect,
      60 + (projected[0] / 256) * 300,
      20 + (projected[1] / 256) * 300,
      world,
      zoomed,
    );
    const dense = aggregateActors(
      Array.from({ length: 100000 }, (_, index) => [
        0,
        8200 + (index % 2),
        8192,
        index % 3,
        index % 3,
      ]).flat(),
      world,
      view,
      140,
    );
    const small = aggregateActors(
      [1, 260, 256, 3, 255],
      [512, 512],
      { zoom: 1, center: [256, 256] },
      140,
    );
    return { bins, reversed, back, dense, small };
  });
  expect(result.bins).toHaveLength(1);
  expect(result.bins[0]).toMatchObject({ owner: 3, count: 3, colour: 255 });
  expect(result.bins[0].size).toBeLessThanOrEqual(1);
  expect(result.reversed).toEqual(result.bins);
  expect(result.dense).toHaveLength(1);
  expect(result.dense[0]).toMatchObject({ owner: 0, count: 100000 });
  expect(result.small[0].size).toBeGreaterThan(result.bins[0].size);
  expect(result.back[0]).toBeCloseTo(6100);
  expect(result.back[1]).toBeCloseTo(7100);
});

test("minimap wheel zoom follows the camera, drag pans the map, click still jumps", async ({
  page,
  request,
}) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected");
  const panel = page.locator("#minimap");
  const map = page.locator("#minimap-map");
  await expect(page.locator("#minimap-entities rect").first()).toBeAttached();
  await page.getByRole("button", { name: "Center on primary unit" }).click();
  await expect(panel).toHaveAttribute("data-camera-x", "8192.5");
  const before = await panel.getAttribute("data-camera-x");
  const bounds = await map.boundingBox();
  if (!bounds) throw new Error("Minimap missing");
  const x = bounds.x + bounds.width / 2,
    y = bounds.y + bounds.height / 2;
  await page.mouse.move(x, y);
  await page.mouse.wheel(0, -900);
  await expect
    .poll(async () => Number(await panel.getAttribute("data-minimap-zoom")))
    .toBeGreaterThan(1);
  await expect(page.locator("#minimap-zoom")).toContainText("×");
  expect(await panel.getAttribute("data-camera-x")).toBe(before);
  const transform = await page
    .locator("#minimap-view")
    .getAttribute("transform");
  await page.mouse.down();
  await page.mouse.move(x + 20, y + 10, { steps: 5 });
  await page.mouse.up();
  await expect(page.locator("#minimap-view")).not.toHaveAttribute(
    "transform",
    transform ?? "",
  );
  expect(await panel.getAttribute("data-camera-x")).toBe(before);
  await page.mouse.click(x + 12, y);
  await expect(panel).not.toHaveAttribute("data-camera-x", before ?? "");
  // Zoom and pan reuse the terrain image, with no detail requests or raster redraw.
  await page.mouse.wheel(0, 10000);
  await expect(panel).toHaveAttribute("data-minimap-zoom", "1");
});
