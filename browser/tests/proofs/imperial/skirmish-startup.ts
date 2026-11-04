import { expect, type Page } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";

/** Opt-in, real-pack France/99-AI measurement through the ordinary setup. */
export async function skirmishStartupProof(
  page: Page,
  url: string,
  label: string,
) {
  const responses: unknown[] = [];
  page.on("response", async (response) => {
    const path = new URL(response.url()).pathname;
    if (!/^\/(skirmish|maps\/)/.test(path)) return;
    const body =
      path === "/skirmish" ? await response.json().catch(() => null) : null;
    responses.push({
      path,
      status: response.status(),
      timing: response.request().timing(),
      body,
    });
  });
  await page.addInitScript(() => {
    performance.setResourceTimingBufferSize(10_000);
    sessionStorage.setItem(
      "aoeworld.skirmish-map",
      JSON.stringify(["46.6", "2.5", "960", "240"]),
    );
    sessionStorage.setItem("aoeworld.skirmish-opponents", "99");
    const events: unknown[] = [];
    Reflect.set(window, "startupEvents", events);
    document.addEventListener("DOMContentLoaded", () => {
      const observe = () => {
        const scene = document.getElementById("scene");
        const minimap = document.getElementById("minimap");
        events.push({
          ms: performance.now(),
          state: document.body.dataset.startState,
          frame: scene?.dataset.frame,
          pending: scene?.dataset.chunksPending,
          failed: scene?.dataset.chunksFailed,
          raster: minimap?.dataset.raster,
          hash: minimap?.dataset.mapHash,
          phase: document.getElementById("loading-phase")?.textContent,
        });
      };
      new MutationObserver(observe).observe(document.body, {
        subtree: true,
        attributes: true,
        attributeFilter: [
          "data-start-state",
          "data-chunks-pending",
          "data-raster",
        ],
        childList: true,
      });
      observe();
    });
  });
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.goto(`${url}?skipIntro=1`);
  await page
    .getByRole("button", { name: "Single Player", exact: true })
    .click();
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 90_000,
  });
  await expect(page.locator("#start-opponents")).toBeEnabled();
  await expect(page.locator("#start-opponents")).toHaveValue("99");
  const started = await page.evaluate(() => performance.now());
  await page.getByRole("button", { name: "Start Game", exact: true }).click();
  let failure: unknown;
  let playable: number | null = null;
  try {
    await expect(page.locator("#start-sequence")).toBeHidden({
      timeout: 300_000,
    });
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-chunks-pending",
      "0",
    );
    playable = await page.evaluate(() => performance.now());
    await mkdir("../.cache/startup", { recursive: true });
    await page.screenshot({ path: `../.cache/startup/${label}-playable.png` });
    // Measure the optional background work separately from the playable frame.
    await expect(page.locator("#minimap")).toHaveAttribute(
      "data-raster",
      "ready",
      { timeout: 180_000 },
    );
  } catch (error) {
    failure = error;
  }
  const result = await page.evaluate(() => ({
    ended: performance.now(),
    events: Reflect.get(window, "startupEvents"),
    resources: performance.getEntriesByType("resource").map((entry) => {
      const resource = entry as PerformanceResourceTiming;
      return {
        path: new URL(entry.name).pathname,
        start: entry.startTime,
        duration: entry.duration,
        bytes: resource.encodedBodySize,
        status: resource.responseStatus,
      };
    }),
  }));
  await mkdir("../.cache/startup", { recursive: true });
  await page.screenshot({ path: `../.cache/startup/${label}.png` });
  await writeFile(
    `../.cache/startup/${label}.json`,
    JSON.stringify({ started, playable, ...result, responses }, null, 2),
  );
  console.log(
    JSON.stringify({
      label,
      startToPlayableMs: playable === null ? null : playable - started,
      completed: !failure,
    }),
  );
  if (failure) throw failure;
}
