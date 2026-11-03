import { expect, type Page } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";
import { PNG } from "pngjs";

/** Cold-context navigation to the first complete presented landscape. */
export async function startupProof(page: Page, url: string, label: string) {
  const profiler = label.endsWith("profile")
    ? await page.context().newCDPSession(page)
    : undefined;
  if (profiler) {
    await profiler.send("Profiler.enable");
    await profiler.send("Profiler.start");
  }
  expect(
    (
      await page.request.post(`${url}maps/reset?scenario=imperial-army`)
    ).status(),
  ).toBe(204);
  if (label.startsWith("trial")) {
    await page.route("**/asset-pack/buildings/**", (route) =>
      route.fulfill({ status: 404 }),
    );
  }
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.goto(`${url}?play=1#x=98&y=134&zoom=.4&renderer=webgpu`);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 90_000,
  });
  let landscape: Buffer | undefined;
  // Presentation counters can describe an earlier empty scene. Read actual
  // world pixels; the measurement is an upper bound including screenshot cost.
  await expect
    .poll(
      async () => {
        const bytes = await page.screenshot();
        const image = PNG.sync.read(bytes);
        let ground = 0;
        for (let y = 60; y < 510; y += 4) {
          for (let x = 0; x < image.width; x += 4) {
            const i = (y * image.width + x) * 4;
            const r = image.data[i] ?? 0,
              g = image.data[i + 1] ?? 0,
              b = image.data[i + 2] ?? 0;
            if (g > 45 && g > r * 0.9 && g > b * 1.2 && r < 230) ground++;
          }
        }
        if (ground > 10_000) landscape = bytes;
        return ground;
      },
      { timeout: 90_000, intervals: [250] },
    )
    .toBeGreaterThan(10_000);
  const result = await page.evaluate(() => ({
    firstLandscapeMs: performance.now(),
    renderer: document.getElementById("playground")?.dataset.renderer,
    assets: performance
      .getEntriesByType("resource")
      .filter((e) => e.name.includes("/asset-pack/"))
      .map((e) => ({
        url: e.name,
        duration: e.duration,
        bytes: (e as PerformanceResourceTiming).encodedBodySize,
      })),
  }));
  await mkdir("../.cache/imperial-art/load", { recursive: true });
  if (profiler) {
    const profile = await profiler.send("Profiler.stop");
    await writeFile(
      `../.cache/imperial-art/load/${label}.cpuprofile`,
      JSON.stringify(profile.profile),
    );
    await profiler.detach();
  }
  if (!landscape) throw new Error("No landscape was presented");
  await writeFile(`../.cache/imperial-art/load/${label}.png`, landscape);
  await writeFile(
    `../.cache/imperial-art/load/${label}.json`,
    JSON.stringify(result, null, 2),
  );
  console.log(JSON.stringify({ label, ...result }));
}
