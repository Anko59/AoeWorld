import { expect, type Page } from "@playwright/test";
import { writeFile } from "node:fs/promises";
import { sampleFrames } from "../rendering/fixtures/bigview.js";

// A real source-backed Paris package, 8,000 tiles per side, prepared locally.
// No scripted sockets, fog, terrain or opponents in this qualification take.
const map = "13445235e81ff032da67e4f01f246b71272d891a71506ccc3a95d3add2cb8de9";

export async function strategicLargeProof(
  page: Page,
  url: string,
  started: number,
) {
  await page.goto(`${url}?play=1&perf=1#renderer=webgl2`);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  const token = await page.evaluate(
    () => localStorage.getItem("aoeworld.resume-token") ?? "",
  );
  const headers = { "X-AoeWorld-Controller-Token": token };
  try {
    const response = await page.request.post(`${url}skirmish`, {
      headers,
      data: { map_hash: map, opponents: 99 },
    });
    expect(response.status()).toBe(202);
    await expect
      .poll(
        async () => {
          const status = (await (
            await page.request.get(`${url}skirmish`)
          ).json()) as { phase: string; error?: string };
          if (status.phase === "failed")
            throw new Error(status.error ?? "Skirmish failed");
          return status.phase;
        },
        { timeout: 420_000, intervals: [1000] },
      )
      .toBe("ready");
    const status = (await (
      await page.request.get(`${url}skirmish`)
    ).json()) as Record<string, unknown>;
    expect(status["opponents"]).toBe(99);
    expect(status["ai_hosts"]).toBeGreaterThan(0);
    await page.reload();
    await expect(page.locator("#connection")).toHaveText("connected", {
      timeout: 60_000,
    });
    await page.keyboard.press("h");
    await expect(page.locator("#minimap")).toHaveAttribute(
      "data-world-width",
      "8000",
    );
    const camera = await page
      .locator("#minimap")
      .evaluate((e) => ({ ...e.dataset }));
    await page.evaluate(
      ({ x, y }) => {
        location.hash = `x=${x}&y=${y}&zoom=.02`;
      },
      { x: camera.cameraX, y: camera.cameraY },
    );
    await expect(page.locator("#scene")).toHaveAttribute("data-lod", "map");
    await expect(page.locator("#minimap")).toHaveAttribute(
      "data-raster",
      "ready",
      { timeout: 60_000 },
    );
    await expect
      .poll(async () =>
        Number(
          await page.locator("#strategic-scene").getAttribute("data-resources"),
        ),
      )
      .toBeGreaterThan(0);
    const ready = (Date.now() - started) / 1000;
    await page.screenshot({ path: "../.cache/showcase/large-first.png" });
    const performance = await sampleFrames(page);
    expect(performance.frameMs).toBeGreaterThan(0);
    await page.keyboard.press("F5");
    await page.waitForTimeout(1500);
    await page.screenshot({ path: "../.cache/showcase/large-middle.png" });
    await page.keyboard.press("F5");
    await page.evaluate(() => {
      location.hash = "x=4000&y=4000&zoom=.0012";
    });
    await page.waitForTimeout(1500);
    await expect
      .poll(async () =>
        Number(
          await page.locator("#strategic-scene").getAttribute("data-resources"),
        ),
      )
      .toBeGreaterThan(0);
    await page.screenshot({ path: "../.cache/showcase/large-last.png" });
    const data = {
      ready,
      duration: (Date.now() - started) / 1000 - ready,
      status,
      performance,
      scene: await page
        .locator("#strategic-scene")
        .evaluate((e) => ({ ...e.dataset })),
      camera: await page
        .locator("#minimap")
        .evaluate((e) => ({ ...e.dataset })),
      health: (await (
        await page.request.get(`${url}health`)
      ).json()) as unknown,
    };
    await writeFile(
      "../.cache/showcase/large-capture.json",
      JSON.stringify(data, null, 2),
    );
  } finally {
    const stopped = await page.request.post(`${url}skirmish/stop`, { headers });
    expect(stopped.ok()).toBe(true);
    const state = (await (await page.request.get(`${url}skirmish`)).json()) as {
      ai_hosts: number;
    };
    expect(state.ai_hosts).toBe(0);
  }
}
