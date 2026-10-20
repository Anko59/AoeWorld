import { expect } from "@playwright/test";
import { readFile, writeFile } from "node:fs/promises";
import { test } from "../../server.js";
import { forceWebGl } from "../rendering/backends.js";

// Local capture of an ordinary, already-built village; no wire/asset fixtures.
test.describe.configure({ mode: "serial" });
for (const label of ["before", "after"] as const) {
  test(`capture the native DAT village (${label}) with all four requested building kinds`, async ({
    page,
  }) => {
    const config = await readFile(
      "../.cache/tc-shots/capture-request.json",
      "utf8",
    )
      .then(
        (text) =>
          JSON.parse(text) as {
            url: string;
            before?: string;
            x: number;
            y: number;
          },
      )
      .catch(() => null);
    test.skip(!config, "Local native village proof was not requested");
    if (!config) return;
    test.setTimeout(120_000);
    const url = label === "before" ? config.before : config.url;
    test.skip(!url, "This native village origin was not requested");
    if (!url) return;
    const origin = new URL(url);
    expect(["127.0.0.1", "localhost"]).toContain(origin.hostname);
    await page.setViewportSize({ width: 1280, height: 720 });
    await forceWebGl(page);
    await page.goto(`${origin}?play=1#x=${config.x}&y=${config.y}&zoom=.7`);
    try {
      await expect(page.locator("#connection")).toHaveText("connected", {
        timeout: 60_000,
      });
    } catch (error) {
      await writeFile(
        "../.cache/tc-shots/village-startup.txt",
        await page.locator("body").innerText(),
      );
      await page.screenshot({ path: "../.cache/tc-shots/village-startup.png" });
      throw error;
    }
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-village-art",
      "native",
      {
        timeout: 60_000,
      },
    );
    await expect(page.locator("#playground")).toHaveAttribute(
      "data-renderer",
      "webgl2",
    );
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-chunks-pending",
      "0",
      {
        timeout: 60_000,
      },
    );
    await expect
      .poll(
        async () =>
          page.locator("#scene").evaluate((canvas) => {
            const actors = JSON.parse(
              canvas.getAttribute("data-actors") ?? "[]",
            ) as {
              kind: string;
              x: number;
              y: number;
            }[];
            return ["TownCenter", "House", "Mill", "Barracks", "Farm"].every(
              (kind) =>
                actors.some(
                  (actor) =>
                    actor.kind === kind &&
                    actor.x > 0 &&
                    actor.x < 1280 &&
                    actor.y > 0 &&
                    actor.y < 600,
                ),
            );
          }),
        { timeout: 60_000 },
      )
      .toBe(true);
    await page.screenshot({ path: `../.cache/tc-shots/village-${label}2.png` });
    await writeFile(
      `../.cache/tc-shots/village-${label}2.json`,
      JSON.stringify(
        await page.locator("#scene").evaluate((canvas) => ({
          actors: JSON.parse(
            canvas.getAttribute("data-actors") ?? "[]",
          ) as unknown,
          atlasFrames: document
            .getElementById("playground")
            ?.getAttribute("data-atlas-frames"),
          renderer: document
            .getElementById("playground")
            ?.getAttribute("data-renderer"),
          map: document
            .getElementById("minimap")
            ?.getAttribute("data-map-hash"),
        })),
        null,
        2,
      ),
    );
    for (const [name, x, y, zoom] of [
      ["tc-close", config.x + 1, config.y + 1, 1],
      ["barracks-close", config.x - 5.5, config.y - 0.5, 1.4],
    ] as const) {
      const previous = Number(
        await page.locator("#scene").getAttribute("data-frame"),
      );
      await page.evaluate(
        ({ x, y, zoom }) => {
          location.hash = `x=${x}&y=${y}&zoom=${zoom}`;
        },
        { x, y, zoom },
      );
      await expect
        .poll(async () =>
          page
            .locator("#minimap")
            .evaluate((map) => [
              Number(map.getAttribute("data-camera-x")),
              Number(map.getAttribute("data-camera-y")),
              Number(map.getAttribute("data-zoom")),
            ]),
        )
        .toEqual([x, y, zoom]);
      await expect(page.locator("#scene")).toHaveAttribute(
        "data-chunks-pending",
        "0",
      );
      await expect(page.locator("#scene")).toHaveAttribute(
        "data-chunks-failed",
        "0",
      );
      await expect
        .poll(async () =>
          Number(await page.locator("#scene").getAttribute("data-frame")),
        )
        .toBeGreaterThan(previous);
      await page.screenshot({
        path: `../.cache/tc-shots/${name}-${label}2.png`,
      });
    }
  });
}
