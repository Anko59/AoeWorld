import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { explorationFixture } from "./exploration-fixture.js";

test.use({ screenshot: "off", trace: "off" });

test("strategic fog reuses one raster without creating decoded image history", async ({
  page,
}) => {
  await explorationFixture(page);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-raster",
    "ready",
  );
  const evidence = await page.evaluate(() => {
    let encodings = 0;
    let allocations = 0;
    const encode = HTMLCanvasElement.prototype.toDataURL;
    const allocate = CanvasRenderingContext2D.prototype.createImageData;
    HTMLCanvasElement.prototype.toDataURL = function (...args) {
      encodings++;
      return encode.apply(this, args);
    };
    CanvasRenderingContext2D.prototype.createImageData = function (
      this: CanvasRenderingContext2D,
      width: number,
      height: number,
    ) {
      allocations++;
      return Reflect.apply(allocate, this, [width, height]) as ImageData;
    } as typeof allocate;
    try {
      const navigation = (
        window as unknown as {
          aoeNavigation: { fog(disabled: boolean, pixels: Uint32Array): void };
        }
      ).aoeNavigation;
      for (let index = 0; index < 4096; index++) {
        navigation.fog(false, Uint32Array.of(index | (2 << 16)));
      }
      navigation.fog(false, Uint32Array.of(3 | (2 << 16), 4 | (1 << 16)));
      const canvas = document.querySelector(
        "#minimap-fog canvas",
      ) as HTMLCanvasElement | null;
      const pixels = canvas?.getContext("2d")?.getImageData(3, 0, 3, 1).data;
      return {
        encodings,
        allocations,
        workingBytes: Number(
          document
            .querySelector("#minimap-fog")
            ?.getAttribute("data-working-bytes") ?? 0,
        ),
        rasterBytes: canvas ? canvas.width * canvas.height * 4 : null,
        alphas: pixels ? [pixels[3], pixels[7], pixels[11]] : null,
      };
    } finally {
      HTMLCanvasElement.prototype.toDataURL = encode;
      CanvasRenderingContext2D.prototype.createImageData = allocate;
    }
  });
  expect(evidence.encodings).toBe(0);
  expect(evidence.allocations).toBeLessThanOrEqual(1);
  expect(evidence.rasterBytes).toBe(256 * 256 * 4);
  expect(evidence.workingBytes).toBe(2 * 256 * 256 * 4);
  expect(evidence.alphas).toEqual([0, 140, 255]);
});
