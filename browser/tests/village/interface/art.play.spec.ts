import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { fiveVillagerFixture } from "../fixtures/village.js";
import { centerVillage, freshHud } from "../fixtures/resources.js";
import { gameAssets } from "../../game-assets.js";

test("Imperial Siege Workshop research remains clickable beneath training", async ({
  page,
  request,
}) => {
  test.setTimeout(90_000);
  expect(
    (await request.post("/maps/reset?scenario=imperial-army")).status(),
  ).toBe(204);
  await gameAssets(page);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected");
  await centerVillage(page, true);
  await page.locator("#research-imperial-age").click();
  await expect(page.locator("#age")).toContainText("Imperial Age", {
    timeout: 25_000,
  });
  await page.evaluate(() => {
    location.hash = "zoom=.35";
  });
  await freshHud(page);
  const site = await page.locator("#scene").evaluate((el) =>
    (
      JSON.parse(el.getAttribute("data-actors") ?? "[]") as {
        kind: string;
        x: number;
        y: number;
      }[]
    ).find((a) => a.kind === "SiegeWorkshop"),
  );
  if (!site) throw new Error("Siege Workshop missing");
  for (const dy of [0, -24, -48, -72]) {
    await page.mouse.click(site.x, site.y + dy);
    await freshHud(page);
    if (
      (await page.locator("#selection").getAttribute("data-kind")) ===
      "SiegeWorkshop"
    )
      break;
  }
  await expect(page.locator("#selection")).toHaveAttribute(
    "data-kind",
    "SiegeWorkshop",
  );
  const train = await page.locator("#train-mangonel").boundingBox();
  const research = await page.locator("#research-capped-ram").boundingBox();
  if (!train || !research) throw new Error("Workshop commands missing");
  expect(research.y).toBeGreaterThanOrEqual(train.y + train.height);
  await page.locator("#research-capped-ram").click({ timeout: 5000 });
  await expect(page.locator("#research-capped-ram")).toBeHidden({
    timeout: 15_000,
  });
});

for (const available of [false, true]) {
  test(`original UI catalog ${available ? "hydrates original art" : "keeps SVG fallback"}`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1280, height: 720 });
    await gameAssets(page, true);
    // Synthetic metadata and pixels only: no original images in this test.
    const art = {
      page: "ui-test.svg",
      x: 0,
      y: 0,
      width: 36,
      height: 36,
      page_width: 36,
      page_height: 36,
    };
    const deArt = { ...art, page: "ui-de.svg" };
    await page.route("**/asset-pack/**", (route) => {
      if (route.request().url().endsWith("interface.json") && !available) {
        return route.fulfill({ status: 404, body: "No local UI catalog" });
      }
      if (route.request().url().endsWith("interface.json") && available) {
        if (route.request().url().endsWith("buildings/interface.json")) {
          return route.fulfill({
            json: {
              units: { 109: deArt },
              technologies: { 22: deArt, 103: deArt },
              commands: { stop: deArt, build: deArt },
              resources: { wood: deArt },
              panels: {
                "western-commands": deArt,
                "western-selection": deArt,
                "western-minimap": deArt,
                "western-top": deArt,
                portrait: deArt,
                button: deArt,
                "menu-background": {
                  ...deArt,
                  page: "ui-de-menu.svg",
                  x: 36,
                  page_width: 72,
                },
              },
            },
          });
        }
        return route.fulfill({
          json: {
            units: { 83: art, 109: art },
            technologies: { 22: art, 101: art },
            commands: { move: art, stop: art, build: art, military: art },
            resources: { wood: art },
            panels: {
              "western-commands": art,
              "western-selection": art,
              "western-minimap": art,
              "western-top": art,
              button: art,
            },
          },
        });
      }
      if (route.request().url().endsWith("ui-test.svg")) {
        return route.fulfill({
          contentType: "image/svg+xml",
          body: '<svg xmlns="http://www.w3.org/2000/svg" width="36" height="36"><rect width="36" height="36" fill="#abc"/></svg>',
        });
      }
      if (route.request().url().endsWith("ui-de.svg")) {
        return route.fulfill({
          contentType: "image/svg+xml",
          body: '<svg xmlns="http://www.w3.org/2000/svg" width="36" height="36"><rect width="36" height="36" fill="#d43"/></svg>',
        });
      }
      if (route.request().url().endsWith("ui-de-menu.svg")) {
        return route.fulfill({
          contentType: "image/svg+xml",
          body: '<svg xmlns="http://www.w3.org/2000/svg" width="72" height="36"><rect width="36" height="36" fill="#0f0"/><rect x="36" width="36" height="36" fill="#d43"/></svg>',
        });
      }
      return route.fallback();
    });
    await fiveVillagerFixture(page);
    await page.goto("/?play=1");
    await expect(page.locator("#connection")).toHaveText("connected");
    await centerVillage(page, true);
    await expect(page.locator("#selection-name")).toHaveText("Town Center");
    await expect(page.locator("#train-villager")).toBeEnabled();
    await expect(page.locator("#research-loom")).toBeVisible();
    if (available) {
      await expect(page.locator("html")).toHaveAttribute(
        "data-interface-art",
        "original",
      );
      await expect(
        page.locator('#selection-portrait [data-ui-id="109"] .original-icon'),
      ).toBeVisible();
      await expect(
        page.locator('#research-loom [data-ui-id="22"] .original-icon'),
      ).toBeVisible();
      await expect(
        page.locator("#research-loom .original-icon image"),
      ).toHaveAttribute("href", "/asset-pack/buildings/ui-de.svg");
      await expect(
        page.locator("#research-imperial-age .original-icon image"),
      ).toHaveAttribute("href", "/asset-pack/buildings/ui-de.svg");
      await expect(
        page.locator('#top-bar [data-ui-id="wood"] .original-icon'),
      ).toBeVisible();
      await expect(
        page.locator('#top-bar [data-ui-id="wood"] .original-icon image'),
      ).toHaveAttribute("href", "/asset-pack/buildings/ui-de.svg");
      for (const selector of [
        "#commands",
        "#selection",
        "#minimap",
        "#top-bar",
        "#selection-portrait",
      ]) {
        await expect(page.locator(selector)).toHaveCSS(
          "background-image",
          /\/asset-pack\/buildings\/ui-de\.svg/,
        );
      }
      await expect(page.locator("html")).toHaveCSS(
        "--ui-button-image",
        /\/asset-pack\/buildings\/ui-de\.svg/,
      );
      await expect(page.locator("html")).toHaveCSS(
        "--ui-menu-image",
        /^url\(["']?data:image\/png;base64,/,
      );
      const menuPixel = await page.evaluate(async () => {
        const value = getComputedStyle(document.documentElement)
          .getPropertyValue("--ui-menu-image")
          .trim();
        const image = new Image();
        image.src = value.replace(/^url\(["']?/, "").replace(/["']?\)$/, "");
        await image.decode();
        const canvas = document.createElement("canvas");
        canvas.width = canvas.height = 1;
        const context = canvas.getContext("2d");
        if (!context) throw new Error("No menu crop canvas");
        context.drawImage(image, 0, 0, 1, 1);
        return Array.from(context.getImageData(0, 0, 1, 1).data);
      });
      expect(menuPixel).toEqual([221, 68, 51, 255]);
      await expect(page.locator("#research-feudal-age")).toBeDisabled();
      await expect(page.locator("#research-feudal-age")).toHaveCSS(
        "filter",
        "none",
      );
      await page.screenshot({ path: "../.cache/showcase/command-colours.png" });
    } else {
      await expect(page.locator("html")).toHaveAttribute(
        "data-interface-art",
        "fallback",
      );
      await expect(
        page.locator("#selection-portrait .fallback-icon"),
      ).toBeVisible();
    }
    const worker = await page.locator("#scene").evaluate((el) =>
      (
        JSON.parse(el.getAttribute("data-actors") ?? "[]") as {
          kind: string;
          x: number;
          y: number;
        }[]
      ).find((a) => a.kind === "Villager"),
    );
    if (!worker) throw new Error("Worker missing");
    await page.mouse.dblclick(worker.x, worker.y);
    await expect(page.locator("#selected-icons .unit-token")).toHaveCount(5);
    if (available) {
      await expect(page.locator("#selected-icons .original-icon")).toHaveCount(
        5,
      );
      await expect(
        page.locator("#stop-units .original-icon image"),
      ).toHaveAttribute("href", "/asset-pack/buildings/ui-de.svg");
      const token = await page
        .locator("#selected-icons .unit-token")
        .first()
        .elementHandle();
      // An unchanged selection must retain its DOM across authoritative refreshes.
      await expect
        .poll(() => page.locator("#scene").getAttribute("data-hud-revision"))
        .not.toBe("0");
      await page.waitForTimeout(600);
      expect(await token?.evaluate((el) => el.isConnected)).toBe(true);
    }
    await page.keyboard.press("b");
    await expect(page.locator("#build-house")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.locator("#build-house")).toBeHidden();
    // Exercise the presentation boundary with the same bounded numeric fields
    // Rust emits for a Barracks. A queue must never inherit Villager art.
    await page.evaluate(() => {
      const buildings = Reflect.get(window, "aoeBuildings") as {
        queue(values: number[][], visible: boolean): void;
        update(values: Float64Array): void;
      };
      const update = buildings.update;
      buildings.update = () => {};
      const hud = Reflect.get(window, "aoeInterface") as {
        numeric(values: Float64Array, status: string): void;
      };
      hud.numeric = () => {};
      const values = new Float64Array(64);
      values[1] = 6;
      values[2] = 1000;
      values[3] = 1000;
      values[4] = 1000;
      values[5] = -1;
      values[6] = -1;
      values[37] = 2;
      values[38] = 1;
      buildings.queue(
        [
          [5, 10, 2],
          [0, 10, 2],
        ],
        true,
      );
      update(values);
    });
    await expect(page.locator("#training-label")).toHaveText(
      "Training Militia · 50%",
    );
    await expect(page.locator('#training-queue [data-ui-id="74"]')).toHaveCount(
      2,
    );
    await expect(page.locator('#training-queue [data-ui-id="83"]')).toHaveCount(
      0,
    );
  });
}

test("mixed Archery Range queue keeps each authoritative unit icon", async ({
  page,
  request,
}) => {
  test.setTimeout(60_000);
  expect(
    (await request.post("/maps/reset?scenario=feudal-military")).status(),
  ).toBe(204);
  await gameAssets(page);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
  await centerVillage(page);
  await page.mouse.move(640, 360);
  await page.mouse.wheel(0, 288);
  await freshHud(page);
  const range = await page.locator("#scene").evaluate((el) =>
    (
      JSON.parse(el.getAttribute("data-actors") ?? "[]") as {
        kind: string;
        x: number;
        y: number;
      }[]
    ).find((actor) => actor.kind === "ArcheryRange"),
  );
  if (!range) throw new Error("Archery Range missing");
  // Native roofs can overlap another building's ground hotspot. Select the
  // Range's unobstructed roof while preserving the queue/icon assertions.
  for (const dy of [0, -24, -48, -72, -96]) {
    await page.mouse.click(range.x, range.y + dy);
    await freshHud(page);
    if (
      (await page.locator("#selection").getAttribute("data-kind")) ===
      "ArcheryRange"
    )
      break;
  }
  await expect(page.locator("#selection")).toHaveAttribute(
    "data-kind",
    "ArcheryRange",
  );
  await page.locator("#train-archer").click();
  await page.locator("#train-skirmisher").click();
  await expect(page.locator("#training-queue .unit-token")).toHaveCount(2);
  await expect(page.locator('#training-queue [data-ui-id="4"]')).toHaveCount(1);
  await expect(page.locator('#training-queue [data-ui-id="7"]')).toHaveCount(1);
});
