import { expect } from "@playwright/test";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { test } from "../../server.js";
import { centerVillage, freshHud } from "../village/fixtures/resources.js";

test.use({ video: { mode: "on", size: { width: 1280, height: 720 } } });

test("local owner playtest footage with imported game art", async ({
  page,
}) => {
  const config = await readFile("../.cache/showcase/capture.json", "utf8").then(
    (text) => JSON.parse(text) as { bug: number; side: "before" | "after" },
    () => null,
  );
  test.skip(!config, "Local original-art recording was not requested");
  if (!config) return;
  test.setTimeout(120_000);
  const pack = await page.request.get("/asset-pack/buildings/manifest.json");
  expect(pack.ok(), "Imported DE building art is required for footage").toBe(
    true,
  );
  expect(
    (await page.request.post("/maps/reset?scenario=feudal-economy")).status(),
  ).toBe(204);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await centerVillage(page, true);
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-village-art",
    "native",
  );
  const at = await page.locator("#minimap").evaluate((map) => ({
    x: Number(map.getAttribute("data-camera-x")),
    y: Number(map.getAttribute("data-camera-y")),
  }));
  const zoom = config.bug === 18 ? 0.1 : 0.045;
  if (config.bug === 18) {
    at.x += 40;
    at.y += 40;
  }
  await page.evaluate(
    ({ x, y, zoom }) => {
      location.hash = `x=${x}&y=${y}&zoom=${zoom}`;
    },
    { ...at, zoom },
  );
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-lod",
    config.bug === 18 ? "sprites" : "markers",
  );
  await freshHud(page);
  await page.evaluate(({ bug, side }) => {
    const caption = document.createElement("div");
    caption.textContent = `#${bug} ${side.toUpperCase()} · inactive yellow domain circles`;
    caption.style.cssText =
      "position:fixed;bottom:220px;left:20px;z-index:100;background:#111e;color:white;padding:8px;font:20px sans-serif";
    document.body.append(caption);
  }, config);
  await mkdir("../.cache/showcase", { recursive: true });
  await writeFile(
    `../.cache/showcase/bug-${config.bug}-${config.side}.json`,
    JSON.stringify(
      await page.evaluate(() => ({
        panels: ["commands", "selection", "minimap"].map((id) => {
          const node = document.getElementById(id);
          if (!node) throw new Error(`Panel ${id} is missing`);
          return {
            id,
            box: node.getBoundingClientRect().toJSON(),
            height: node.style.getPropertyValue("--frame-height"),
          };
        }),
        domains: document.getElementById("steward-domain")?.outerHTML,
      })),
      null,
      2,
    ),
  );
  await page.screenshot({
    path: `../.cache/showcase/bug-${config.bug}-${config.side}.png`,
  });
  // Intentional recorded screen time after all readiness checks.
  await page.waitForTimeout(2500);
  const video = page.video();
  await page.close();
  if (!video) throw new Error("Recording was not created");
  await video.saveAs(
    `../.cache/showcase/bug-${config.bug}-${config.side}-raw.webm`,
  );
});
