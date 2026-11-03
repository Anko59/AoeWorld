import { expect } from "@playwright/test";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { fallbackTest as test, forceWebGl } from "./backends.js";

test("original-art cockpit proof", async ({ browser, baseURL }, info) => {
  test.skip(
    info.project.name !== "browser-defaults",
    "Normal WebGL compositor take",
  );
  const config = await readFile("../.cache/showcase/cockpit.json", "utf8").then(
    (text) => JSON.parse(text) as { stage: string; scenario?: string },
    () => null,
  );
  test.skip(!config, "Opt-in local original-art proof");
  if (!config) return;
  test.setTimeout(
    config.stage === "item2" || config.stage === "item3" ? 180_000 : 120_000,
  );
  await mkdir("../.cache/showcase", { recursive: true });
  const context = await browser.newContext({
    baseURL,
    viewport: { width: 1280, height: 720 },
    recordVideo: {
      dir: "../.cache/showcase/raw",
      size: { width: 1280, height: 720 },
    },
  });
  const page = await context.newPage();
  const started = Date.now();
  let ready: number;
  const observations: Record<string, unknown> = {};
  const actors = async () =>
    JSON.parse(
      (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
    ) as { id: number; kind: string; x: number; y: number }[];
  await forceWebGl(page);
  try {
    expect(
      (
        await page.request.post(
          `/maps/reset?scenario=${config.scenario ?? "general-army"}`,
        )
      ).status(),
    ).toBe(204);
    await page.goto(
      config.stage === "item3"
        ? "/?play=1#x=128&y=130&zoom=.5&renderer=webgl2"
        : "/?play=1#x=106&y=128&zoom=.5&renderer=webgl2",
    );
    await expect(page.locator("#connection")).toHaveText("connected", {
      timeout: 60_000,
    });
    await expect(page.locator("html")).toHaveAttribute(
      "data-interface-art",
      "original",
    );
    await expect(page.locator("#playground")).toHaveAttribute(
      "data-renderer",
      "webgl2",
    );
    if (!config.scenario || config.scenario === "general-army") {
      await expect
        .poll(async () => {
          const actors = JSON.parse(
            (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
          ) as { kind: string; x: number; y: number }[];
          const castle = actors.find((a) => a.kind === "Castle");
          if (!castle) return false;
          await page.mouse.click(castle.x, castle.y);
          return page.locator("#train-general").isVisible();
        })
        .toBe(true);
      await page.locator("#train-general").click();
      await expect
        .poll(
          async () =>
            (
              await page.locator("#scene").getAttribute("data-actors")
            )?.includes('"General"'),
          { timeout: 30_000 },
        )
        .toBe(true);
    }
    if (config.stage === "item3") {
      await page.locator("#idle-villagers").click();
      await expect(page.locator("#selection")).toHaveAttribute(
        "data-kind",
        "Villager",
      );
      await page.locator("#build-military-menu").click();
      await page.locator("#build-barracks").click();
      let placed = false;
      for (const [x, y] of [
        [850, 310],
        [850, 400],
        [420, 300],
        [430, 400],
        [700, 420],
      ]) {
        if (x === undefined || y === undefined) continue;
        await page.mouse.move(x, y);
        await page.waitForTimeout(350);
        if (
          (await page
            .locator("#scene")
            .getAttribute("data-placement-valid")) !== "true"
        )
          continue;
        await page.mouse.click(x, y);
        placed = true;
        break;
      }
      expect(placed).toBe(true);
      await expect
        .poll(
          async () =>
            (await actors()).filter((a) => a.kind === "Barracks").length,
        )
        .toBe(2);
      // Inspect the new foundation using the ordinary selection panel.
      const barracks = (await actors())
        .filter((a) => a.kind === "Barracks")
        .sort((a, b) => b.id - a.id)[0];
      if (!barracks) throw new Error("New Barracks missing");
      await page.mouse.click(barracks.x, barracks.y);
      await expect(page.locator("#selection")).toHaveAttribute(
        "data-kind",
        "Barracks",
      );
      await expect(page.locator("#selection")).toContainText("Complete", {
        timeout: 45_000,
      });
      await page.keyboard.press("Escape");
    }
    await page.waitForTimeout(1200);
    ready = (Date.now() - started) / 1000;
    await page.screenshot({
      path: `../.cache/showcase/${config.stage}-near.png`,
    });
    await page.evaluate(() => {
      location.hash = "x=128&y=128&zoom=.04";
    });
    await expect(page.locator("#scene")).toHaveAttribute("data-lod", "markers");
    await page.waitForTimeout(1200);
    await page.screenshot({
      path: `../.cache/showcase/${config.stage}-far.png`,
    });
    if (config.stage.startsWith("item1")) {
      await expect(page.locator("#strategic-cockpit")).toBeVisible();
      await expect(
        page.locator("#strategic-cockpit .original-icon").first(),
      ).toBeVisible();
      await page.waitForTimeout(3000);
      await page.screenshot({
        path: `../.cache/showcase/${config.stage}-last.png`,
      });
      if (config.scenario === "vassal-kingdom") {
        await expect
          .poll(async () =>
            (await page.locator("[data-income]").allTextContents()).some(
              (t) => Number(t.split(" / ")[0]?.replaceAll(",", "")) > 0,
            ),
          )
          .toBe(true);
        await page.screenshot({
          path: `../.cache/showcase/${config.stage}-last.png`,
        });
      }
    }
    if (config.stage === "item2") {
      await page.keyboard.press("Control+F1");
      await page.evaluate(() => {
        location.hash = "x=180&y=180&zoom=.25";
      });
      await expect(page.locator("#minimap")).toHaveAttribute(
        "data-zoom",
        "0.25",
      );
      await page.keyboard.press("Shift+F1");
      await expect(page.locator("#minimap")).toHaveAttribute(
        "data-zoom",
        "0.04",
      );
      await expect(page.locator("#minimap")).toHaveAttribute(
        "data-camera-x",
        "128",
      );
      await page.screenshot({ path: "../.cache/showcase/item2-recalled.png" });
      await page.keyboard.press("h");
      await expect(page.locator("#selection")).toHaveAttribute(
        "data-kind",
        "TownCenter",
      );
      observations["base"] = await page
        .locator("#minimap")
        .evaluate((e) => ({ ...e.dataset }));
      await page.waitForTimeout(1000);
      await page.keyboard.press("End");
      await page.waitForTimeout(1000);
      observations["army"] = await page
        .locator("#minimap")
        .evaluate((e) => ({ ...e.dataset }));
      await page.screenshot({ path: "../.cache/showcase/item2-army.png" });
      await page.locator('.army-token[data-owned="true"]').first().click();
      const target = await page.locator("#minimap").evaluate((e) => {
        const cx = Number(e.dataset.cameraX),
          cy = Number(e.dataset.cameraY),
          z = Number(e.dataset.zoom);
        return {
          x: 640 + (166.5 - cx - 130.5 + cy) * 64 * z,
          y: 360 + (166.5 - cx + 130.5 - cy) * 32 * z,
        };
      });
      await page.mouse.click(target.x, target.y, { button: "right" });
      await expect(
        page.getByRole("button", { name: "Last alert Space", exact: true }),
      ).toBeEnabled({ timeout: 45_000 });
      await page.keyboard.press("Space");
      await expect
        .poll(async () =>
          Number(await page.locator("#minimap").getAttribute("data-camera-x")),
        )
        .toBeGreaterThan(140);
      observations["alert"] = await page
        .locator("#minimap")
        .evaluate((e) => ({ ...e.dataset }));
      await page.screenshot({ path: "../.cache/showcase/item2-alert.png" });
      await page.waitForTimeout(1500);
      await page.keyboard.press("Shift+F1");
      await expect(page.locator("#minimap")).toHaveAttribute(
        "data-camera-x",
        "128",
      );
      await page.waitForTimeout(1500);
    }
    if (config.stage === "item3") {
      await page.getByLabel("Unit type").selectOption("2");
      await page.getByLabel("Unit count").fill("6");
      await page
        .getByRole("button", { name: "Units here", exact: true })
        .click();
      await page.mouse.click(640, 390);
      await expect(page.locator("#units-here-status")).toContainText(
        "Requested 6",
      );
      await page.screenshot({ path: "../.cache/showcase/item3-requested.png" });
      await page.evaluate(() => {
        location.hash = "x=128&y=130&zoom=.4";
      });
      await expect(page.locator("#scene")).toHaveAttribute(
        "data-lod",
        "sprites",
      );
      await expect
        .poll(
          async () =>
            (await actors()).filter((a) => a.kind === "Barracks").length,
        )
        .toBe(2);
      const queues = [];
      for (const b of (await actors()).filter((a) => a.kind === "Barracks")) {
        await page.mouse.click(b.x, b.y);
        await expect(page.locator("#selection")).toHaveAttribute(
          "data-kind",
          "Barracks",
        );
        await expect(page.locator("#scene")).toHaveAttribute(
          "data-rally-point",
          /.+/,
        );
        await expect(
          page.locator("#training-queue .unit-token"),
        ).not.toHaveCount(0);
        queues.push({
          id: b.id,
          count: await page.locator("#training-queue .unit-token").count(),
          rally: await page.locator("#scene").getAttribute("data-rally-point"),
        });
        await page.screenshot({
          path: `../.cache/showcase/item3-queue-${b.id}.png`,
        });
      }
      expect(queues).toHaveLength(2);
      const [first, second] = queues;
      if (!first || !second)
        throw new Error("Two producer queues are required");
      expect(first.rally).toBe(second.rally);
      observations["queues"] = queues;
      await page.waitForTimeout(6000);
      await page.screenshot({
        path: "../.cache/showcase/item3-trained-near.png",
      });
      await page.evaluate(() => {
        location.hash = "x=128&y=130&zoom=.04";
      });
      await expect(page.locator(".cockpit-army")).toContainText("Militia: 6", {
        timeout: 40_000,
      });
      await page.waitForTimeout(1500);
      await page.screenshot({ path: "../.cache/showcase/item3-last.png" });
    }
    await writeFile(
      `../.cache/showcase/${config.stage}.json`,
      JSON.stringify(
        {
          ready,
          observations,
          duration: (Date.now() - started) / 1000 - ready,
          camera: await page
            .locator("#minimap")
            .evaluate((e) => ({ ...e.dataset })),
          cockpit: (await page.locator("#strategic-cockpit").count())
            ? await page.locator("#strategic-cockpit").textContent()
            : null,
          health: (await (await page.request.get("/health")).json()) as unknown,
        },
        null,
        2,
      ),
    );
  } finally {
    await context.close();
    await page.video()?.saveAs(`../.cache/showcase/${config.stage}.webm`);
  }
});
