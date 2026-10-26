import { expect, type Page } from "@playwright/test";

export async function overviewTerrain(page: Page) {
  const raster = Buffer.alloc(8 + 64 * 64, 0x80);
  raster.write("AWMM", 0, "latin1");
  raster.writeUInt16LE(64, 4);
  raster[6] = raster[7] = 0;
  let requests = 0;
  await page.route(`**/maps/${"01".repeat(32)}/minimap`, (route) => {
    requests++;
    return route.fulfill({
      contentType: "application/octet-stream",
      body: raster,
    });
  });
  await page.evaluate(async () => {
    const navigation = (
      window as Window & {
        aoeNavigation?: {
          map(hash: Uint8Array): Promise<void>;
          fog(disabled: boolean, pixels: number[]): void;
        };
      }
    ).aoeNavigation;
    if (!navigation) throw new Error("Production navigation missing");
    await navigation.map(new Uint8Array(32).fill(1));
    navigation.fog(true, []);
  });
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-raster",
    "ready",
  );
  return () => requests;
}

export async function zoom(page: Page, delta: number, lod: string) {
  await page.mouse.move(640, 320);
  await page.mouse.wheel(0, delta);
  await expect(page.locator("#scene")).toHaveAttribute("data-lod", lod);
}

export async function sampleFrames(page: Page) {
  return page.evaluate(async () => {
    const perf = (
      window as Window & {
        aoePerformance?: {
          frameMs: number;
          cpuMs: { draw: number; ui: number };
        };
      }
    ).aoePerformance;
    if (!perf) throw new Error("Production perf overlay missing");
    const frames: number[] = [],
      draw: number[] = [],
      ui: number[] = [];
    for (let index = 0; index < 40; index++) {
      await new Promise(requestAnimationFrame);
      if (index < 10) continue;
      frames.push(perf.frameMs);
      draw.push(perf.cpuMs.draw);
      ui.push(perf.cpuMs.ui);
    }
    const median = (values: number[]) =>
      values.sort((a, b) => a - b)[Math.floor(values.length / 2)];
    return {
      frameMs: median(frames),
      drawMs: median(draw),
      uiMs: median(ui),
      frameP95Ms: [...frames].sort((a, b) => a - b)[
        Math.floor(frames.length * 0.95)
      ],
      drawP95Ms: [...draw].sort((a, b) => a - b)[
        Math.floor(draw.length * 0.95)
      ],
      samples: frames.length,
    };
  });
}
