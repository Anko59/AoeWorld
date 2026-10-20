import { expect } from "@playwright/test";
import { readFile, writeFile } from "node:fs/promises";
import { test } from "../../server.js";
import { forceWebGl } from "../rendering/backends.js";

// Local capture of an ordinary, already-built village; no wire/asset fixtures.
test("capture the native DAT village with all four requested building kinds", async ({
  page,
}) => {
  const config = await readFile(
    "../.cache/tc-shots/capture-request.json",
    "utf8",
  )
    .then((text) => JSON.parse(text) as { url: string; x: number; y: number })
    .catch(() => null);
  test.skip(!config, "Local native village proof was not requested");
  if (!config) return;
  test.setTimeout(120_000);
  const origin = new URL(config.url);
  expect(["127.0.0.1", "localhost"]).toContain(origin.hostname);
  await page.setViewportSize({ width: 1280, height: 720 });
  await forceWebGl(page);
  await page.goto(`${origin}?play=1#x=${config.x}&y=${config.y}&zoom=.7`);
  try {
    await expect(page.locator("#connection")).toHaveText("connected", {
      timeout: 60_000,
    });
  } finally {
    await writeFile(
      "../.cache/tc-shots/village-startup.txt",
      await page.locator("body").innerText(),
    );
    await page.screenshot({ path: "../.cache/tc-shots/village-startup.png" });
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
  await page.screenshot({ path: "../.cache/tc-shots/village-after.png" });
  await writeFile(
    "../.cache/tc-shots/village-capture.json",
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
        map: document.getElementById("minimap")?.getAttribute("data-map-hash"),
      })),
      null,
      2,
    ),
  );
});
