import { expect } from "@playwright/test";
import { PNG } from "pngjs";
import { fallbackTest } from "../../rendering/backends.js";
import { building, inputFixture, screen } from "../input/fixture.js";

function greenShare(image: PNG, center: { x: number; y: number }) {
  let green = 0;
  const top = Math.round(center.y) - 12;
  const left = Math.round(center.x) - 24;
  for (let y = top; y < top + 24; y++) {
    for (let x = left; x < left + 48; x++) {
      const i = 4 * (y * image.width + x);
      if (image.data.readUInt8(i + 1) > image.data.readUInt8(i) + 4) green++;
    }
  }
  return green / (48 * 24);
}

fallbackTest(
  "unseeded and planted farms show distinct native terrain",
  async ({ page, request }, info) => {
    fallbackTest.setTimeout(150_000);
    fallbackTest.skip(
      !(await request.head("/asset-pack/buildings/manifest.json")).ok(),
      "Requires the privately imported DE overlay",
    );
    await page.setViewportSize({ width: 1280, height: 720 });
    await inputFixture(
      page,
      "webgl2",
      [
        building(20, 0, 8190, 8190, 4),
        building(21, 5, 8188, 8195, 3, 200, false, 0, 175),
        building(22, 5, 8191, 8195, 3, 900, true, 0, 175),
      ],
      false,
      0,
      undefined,
      { originalArt: true },
    );
    const scene = page.locator("#scene");
    await expect(scene).toHaveAttribute("data-village-art", "native");
    await expect(scene).toHaveAttribute("data-chunks-pending", "0", {
      timeout: 60_000,
    });
    const bare = screen(8189.5, 8196.5);
    const seeded = screen(8192.5, 8196.5);
    await expect
      .poll(
        async () => {
          const image = PNG.sync.read(await page.screenshot({ scale: "css" }));
          return greenShare(image, seeded) - greenShare(image, bare);
        },
        { timeout: 30_000 },
      )
      .toBeGreaterThan(0.25);
    await page.screenshot({
      path: info.outputPath("farm-and-town-center.png"),
    });
  },
);
