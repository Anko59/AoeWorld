import { expect } from "@playwright/test";
import { PNG } from "pngjs";
import { test } from "../../server.js";

// Native ids resolve through DAT recipes; the fixture supplies two compositions.
const slps = [109, 82];
const solid = (rgba: [number, number, number, number]) => {
  const png = new PNG({ width: 128, height: 128 });
  for (let i = 0; i < png.data.length; i += 4) png.data.set(rgba, i);
  return PNG.sync.write(png);
};

test("the far view paints terrain and real building sprites, not a black plane with chips", async ({
  page,
}) => {
  const frames = slps.map((slp) => ({
    source: `fixture:${slp}`,
    frame: 0,
    native: null,
    page: 0,
    x: 0,
    y: 0,
    width: 96,
    height: 96,
    anchor_x: 48,
    anchor_y: 80,
  }));
  await page.route("**/asset-pack/buildings/manifest.json", (route) =>
    route.fulfill({
      json: {
        frames,
        pages: [{ color: "color-000.png", player: "player-000.png" }],
      },
    }),
  );
  await page.route("**/asset-pack/buildings/color-000.png", (route) =>
    route.fulfill({
      contentType: "image/png",
      body: solid([200, 160, 90, 255]),
    }),
  );
  await page.route("**/asset-pack/buildings/player-000.png", (route) =>
    route.fulfill({ contentType: "image/png", body: solid([200, 0, 0, 255]) }),
  );
  await page.route("**/pkg/aoe_client.js", (route) =>
    route.fulfill({
      contentType: "text/javascript",
      body: "export function strategic_building_parts() { return new Float64Array([109,1,0,0,0,82,1,1,0,0]); }",
    }),
  );
  await page.route("**/landscape-test", (route) =>
    route.fulfill({
      contentType: "text/html",
      body: '<link rel="stylesheet" href="/interface/game.css"><canvas id="scene" style="width:1280px;height:720px"></canvas><section id="bottom-panel"><div id="commands"></div><div id="selection" data-count="0"></div></section>',
    }),
  );
  await page.goto("/landscape-test");
  const state = [8000, 8000, 500, 500, 1280, 720, 0.04, 1];
  await page.evaluate(async (frame) => {
    // @ts-expect-error Served browser module.
    const { paintTerrain } = await import("/navigation/landscape/terrain.js");
    // @ts-expect-error Served browser module.
    const { initStrategic } = await import("/navigation/strategic.js");
    document.documentElement.dataset.interfaceArt = "original";
    const s = initStrategic(
      { disabled: true, canvas: document.createElement("canvas"), revision: 0 },
      { records: () => [], revision: () => 0 },
    );
    // Grass everywhere: a raster is available, so the plane must not be black.
    const raster = paintTerrain({
      samples: 64,
      cells: new Uint8Array(64 * 64).fill(0x80),
    });
    (window as unknown as { rasterPixel: number[] }).rasterPixel = Array.from(
      raster.getContext("2d").getImageData(8, 8, 1, 1).data,
    ) as number[];
    s.terrain(raster);
    s.settlements([
      10, 498, 498, 502, 502, 0, 0x4191f5, 1, 109, 11, 506, 500, 509, 503, 5,
      0x4191f5, 1, 50, 12, 490, 500, 494, 504, 16, 0x4191f5, 0, 82,
    ]);
    (window as unknown as { farFrame: () => void }).farFrame = () =>
      s.frame(frame);
    s.frame(frame);
  }, state);
  const canvas = page.locator("#strategic-scene");
  // Sprites bake off the critical path, then the model redraws itself.
  await expect
    .poll(async () => Number(await canvas.getAttribute("data-sprites")), {
      timeout: 15_000,
    })
    .toBeGreaterThan(0);
  expect(Number(await canvas.getAttribute("data-settlements"))).toBeGreaterThan(
    0,
  );
  const lit = await canvas.evaluate((node: HTMLCanvasElement) => {
    const data = node
      .getContext("2d")
      ?.getImageData(0, 0, node.width, node.height).data;
    if (!data) return 0;
    let bright = 0,
      total = 0;
    for (let i = 0; i < data.length; i += 4 * 97) {
      total++;
      if ((data[i] ?? 0) + (data[i + 1] ?? 0) + (data[i + 2] ?? 0) > 60)
        bright++;
    }
    return bright / total;
  });
  expect(lit).toBeGreaterThan(0.9);
  // A corner far from every place shows the raster's own colour, not the clear colour.
  const [corner, raster] = await Promise.all([
    canvas.evaluate((node: HTMLCanvasElement) =>
      Array.from(node.getContext("2d")?.getImageData(20, 20, 1, 1).data ?? []),
    ),
    page.evaluate(
      () => (window as unknown as { rasterPixel: number[] }).rasterPixel,
    ),
  ]);
  for (let i = 0; i < 3; i++)
    expect(Math.abs((corner[i] ?? 0) - (raster[i] ?? 0))).toBeLessThan(40);
  expect(
    raster.reduce((sum, v) => sum + v, 0) - (raster[3] ?? 0),
  ).toBeGreaterThan(60);
});
