import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { fiveVillagerFixture } from "../fixtures/village.js";
import { centerVillage, freshHud } from "../fixtures/resources.js";
import { gameAssets } from "../../game-assets.js";

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
    await page.route("**/asset-pack/**", (route) => {
      if (route.request().url().endsWith("interface.json") && !available) {
        return route.fulfill({ status: 404, body: "No local UI catalog" });
      }
      if (route.request().url().endsWith("interface.json") && available) {
        return route.fulfill({
          json: {
            units: { 83: art, 109: art },
            technologies: { 22: art },
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
        page.locator('#top-bar [data-ui-id="wood"] .original-icon'),
      ).toBeVisible();
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
  await page.mouse.click(range.x, range.y);
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
