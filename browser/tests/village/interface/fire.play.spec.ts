import { expect } from "@playwright/test";
import { PNG } from "pngjs";
import { fallbackTest } from "../../rendering/backends.js";
import { building, inputFixture } from "../input/fixture.js";

function flamePixels(image: PNG, healthy: PNG) {
  let count = 0;
  // The Town Center is at the center of the fixture; exclude the HUD.
  for (let y = 160; y < 375; y++) {
    for (let x = 480; x < 800; x++) {
      const i = 4 * (y * image.width + x);
      const r = image.data.readUInt8(i);
      const g = image.data.readUInt8(i + 1);
      const b = image.data.readUInt8(i + 2);
      if (
        r > 180 &&
        g > 75 &&
        r > g * 1.15 &&
        b < g * 0.75 &&
        r > healthy.data.readUInt8(i) + 45
      )
        count++;
    }
  }
  return count;
}

fallbackTest(
  "native Imperial Town Center fire is visible after damage",
  async ({ context, request }, info) => {
    fallbackTest.setTimeout(240_000);
    fallbackTest.skip(
      !(await request.get("/asset-pack/buildings/actors/manifest.json")).ok(),
      "Requires the privately imported DE overlay",
    );
    const openTown = async (hp: number) => {
      const page = await context.newPage();
      await page.setViewportSize({ width: 1280, height: 720 });
      const town = building(20, 0, 8190, 8190, 4, hp, true, 0, undefined, 3);
      await inputFixture(page, "webgl2", [town], false, 0, undefined, {
        originalArt: true,
      });
      const scene = page.locator("#scene");
      await expect(scene).toHaveAttribute("data-village-art", "native");
      await expect(scene).toHaveAttribute("data-chunks-pending", "0", {
        timeout: 60_000,
      });
      await page.keyboard.press("Escape");
      return page;
    };
    const intact = await openTown(900);
    const healthy = PNG.sync.read(
      await intact.screenshot({
        scale: "css",
        path: info.outputPath("healthy.png"),
      }),
    );
    await intact.close();
    const damaged = await openTown(225);
    await expect
      .poll(
        async () =>
          flamePixels(
            PNG.sync.read(await damaged.screenshot({ scale: "css" })),
            healthy,
          ),
        {
          timeout: 30_000,
          message:
            "native fire adds visible orange pixels around the damaged roof",
        },
      )
      .toBeGreaterThan(75);
    await damaged.screenshot({
      scale: "css",
      path: info.outputPath("damaged.png"),
    });
  },
);
