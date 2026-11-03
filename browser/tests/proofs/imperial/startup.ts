import { expect, type Page } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";

/** Cold-context navigation to the first complete presented landscape. */
export async function startupProof(page: Page, url: string, label: string) {
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
  await page.addInitScript(() => {
    const observe = () => {
      const scene = document.getElementById("scene");
      if (!scene) return;
      const observer = new MutationObserver(() => {
        if (
          document.getElementById("connection")?.textContent === "connected" &&
          Number(scene.dataset.frame) > 0 &&
          scene.dataset.chunksPending === "0" &&
          scene.dataset.chunksFailed === "0"
        ) {
          Reflect.set(window, "firstLandscapeMs", performance.now());
          observer.disconnect();
        }
      });
      observer.observe(scene, { attributes: true });
    };
    document.addEventListener("DOMContentLoaded", observe, { once: true });
  });
  await page.goto(`${url}?play=1#x=98&y=134&zoom=.4&renderer=webgpu`);
  await expect
    .poll(() => page.evaluate(() => Reflect.get(window, "firstLandscapeMs")), {
      timeout: 90_000,
    })
    .toBeGreaterThan(0);
  const result = await page.evaluate(() => ({
    firstLandscapeMs: Reflect.get(window, "firstLandscapeMs"),
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
  await page.screenshot({ path: `../.cache/imperial-art/load/${label}.png` });
  await writeFile(
    `../.cache/imperial-art/load/${label}.json`,
    JSON.stringify(result, null, 2),
  );
  console.log(JSON.stringify({ label, ...result }));
}
