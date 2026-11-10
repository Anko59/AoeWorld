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
  const scenario = config.bug === 14 ? "" : "?scenario=feudal-economy";
  expect((await page.request.post(`/maps/reset${scenario}`)).status()).toBe(
    204,
  );
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
  const camera = async (x: number, y: number, zoom: number) => {
    await page.evaluate(
      (value) => {
        location.hash = `x=${value.x}&y=${value.y}&zoom=${value.zoom}`;
      },
      { x, y, zoom },
    );
    await expect(page.locator("#minimap")).toHaveAttribute(
      "data-camera-x",
      String(x),
    );
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-chunks-pending",
      "0",
    );
    await freshHud(page);
  };
  if (config.bug === 14) {
    await camera(at.x, at.y, 0.5);
    const workers = JSON.parse(
      (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
    ) as { kind: string; x: number; y: number }[];
    const worker = workers.find(
      (a) =>
        a.kind === "Villager" &&
        a.x > 30 &&
        a.x < 1200 &&
        a.y > 70 &&
        a.y < 480,
    );
    if (!worker) throw new Error("No visible builder in the village");
    await page.mouse.click(worker.x, worker.y);
    await expect(page.locator("#selection-name")).toContainText("Villager");
    // Stop the builder so this real site stays unfinished throughout the take.
    await page.locator("#build-menu").click();
    await page.locator("#build-house").click();
    let site: number[] | undefined;
    for (const point of [
      [1000, 360],
      [940, 360],
      [900, 360],
      [850, 280],
      [850, 400],
      [450, 400],
      [450, 280],
      [640, 470],
    ]) {
      await page.mouse.move(point[0] ?? 0, point[1] ?? 0);
      await freshHud(page);
      if (
        (await page.locator("#scene").getAttribute("data-placement-valid")) ===
        "true"
      ) {
        site = point;
        break;
      }
    }
    if (!site)
      throw new Error("No legal house site around the starting village");
    await page.mouse.click(site[0] ?? 0, site[1] ?? 0);
    await page.locator("#build-menu").click();
    await page.locator("#stop-units").click();
    await expect
      .poll(() => page.locator("#construction-labels span").count())
      .toBeGreaterThan(0);
  }
  const zoom = config.bug === 18 ? 0.1 : config.bug === 14 ? 0.099 : 0.045;
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
  if (config.bug !== 18) {
    await expect
      .poll(() =>
        page.evaluate(() => sessionStorage.getItem("aoeworld.cockpit-reveal")),
      )
      .not.toBeNull();
    const skip = page.getByRole("button", { name: "Skip", exact: true });
    if (await skip.isVisible()) await skip.click();
    await expect(page.locator("html")).not.toHaveAttribute(
      "data-cockpit-reveal",
      /.+/,
    );
    await expect
      .poll(
        async () =>
          Number(
            await page.locator("#strategic-scene").getAttribute("data-sprites"),
          ),
        { timeout: 15_000 },
      )
      .toBeGreaterThan(0);
  }
  if (config.bug === 14) {
    const labels = page.locator("#construction-labels");
    if (config.side === "before") await expect(labels).toBeVisible();
    else await expect(labels).toBeHidden();
  }
  await page.evaluate(({ bug, side }) => {
    const caption = document.createElement("div");
    const description =
      bug === 14
        ? "unfinished buildings and world progress"
        : "inactive yellow domain circles";
    caption.textContent = `#${bug} ${side.toUpperCase()} · ${description}`;
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
        construction: document.getElementById("construction-labels")?.outerHTML,
        settlements: document.getElementById("strategic-settlements")
          ?.outerHTML,
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
  if (config.bug === 14)
    await expect
      .poll(() => page.locator("#construction-labels span").count())
      .toBeGreaterThan(0);
  const video = page.video();
  await page.close();
  if (!video) throw new Error("Recording was not created");
  await video.saveAs(
    `../.cache/showcase/bug-${config.bug}-${config.side}-raw.webm`,
  );
});
