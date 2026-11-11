import { expect } from "@playwright/test";
import { PNG } from "pngjs";
import { test } from "../../server.js";

test("far thumbnails distinguish observed Dark and Imperial building recipes", async ({
  page,
}) => {
  const png = new PNG({ width: 4, height: 2 });
  for (let y = 0; y < 2; y++)
    for (let x = 0; x < 4; x++)
      png.data.set(
        x < 2 ? [220, 50, 20, 255] : [20, 80, 220, 255],
        (y * 4 + x) * 4,
      );
  const frames = [0, 2].map((x) => ({
    source: `fixture:${x}`,
    frame: 0,
    native: null,
    page: 0,
    x,
    y: 0,
    width: 2,
    height: 2,
    anchor_x: 1,
    anchor_y: 2,
  }));
  await page.route("**/asset-pack/buildings/manifest.json", (route) =>
    route.fulfill({
      json: {
        frames,
        pages: [
          { color: "color.png", player: "player.png", shadow: "shadow.png" },
        ],
      },
    }),
  );
  await page.route("**/asset-pack/buildings/color.png", (route) =>
    route.fulfill({ contentType: "image/png", body: PNG.sync.write(png) }),
  );
  await page.route("**/asset-pack/buildings/player.png", (route) =>
    route.fulfill({
      contentType: "image/png",
      body: PNG.sync.write(new PNG({ width: 4, height: 2 })),
    }),
  );
  await page.route("**/pkg/aoe_client.js", (route) =>
    route.fulfill({
      contentType: "text/javascript",
      body: "export function strategic_building_parts() { return new Float64Array([109,1,0,0,0,142,1,1,0,0]); }",
    }),
  );
  await page.route("**/age-test", (route) =>
    route.fulfill({ contentType: "text/html", body: "<body></body>" }),
  );
  await page.goto("/age-test");
  const colours = await page.evaluate(async () => {
    // @ts-expect-error Served browser module.
    const sprites = await import("/navigation/landscape/sprites.js");
    sprites.loadSprites();
    await new Promise<void>((resolve) => sprites.onSprites(resolve));
    const sample = async (id: number) => {
      let image = sprites.buildingSprite(id, 0x4191f5);
      if (!image) {
        await new Promise<void>((resolve) => sprites.onSprites(resolve));
        image = sprites.buildingSprite(id, 0x4191f5);
      }
      if (!image) throw new Error(`No far art for native building ${id}`);
      return Array.from(
        image.canvas.getContext("2d").getImageData(0, 0, 1, 1).data,
      );
    };
    return [await sample(109), await sample(142)];
  });
  expect(colours).toEqual([
    [220, 50, 20, 255],
    [20, 80, 220, 255],
  ]);
});
