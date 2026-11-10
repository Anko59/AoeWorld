import { expect } from "@playwright/test";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { test } from "../../server.js";
import { centerVillage, freshHud } from "../village/fixtures/resources.js";

import { contactProof } from "./contact-proof.js";

type Capture = { bug: number; side: "before" | "after" };
for (const bug of [18, 14, 9, 8, 6, 16, 12]) {
  test(`local owner playtest footage with imported game art #${bug}`, async ({
    playwright,
    browserName,
    launchOptions,
    headless,
    channel,
    baseURL,
  }) => {
    const requested = await readFile(
      "../.cache/showcase/capture.json",
      "utf8",
    ).then(
      (text) => JSON.parse(text) as Capture | Capture[],
      () => null,
    );
    const config = Array.isArray(requested)
      ? requested.find((c) => c.bug === bug)
      : requested?.bug === bug
        ? requested
        : null;
    test.skip(!config, "Local original-art recording was not requested");
    if (!config) return;
    test.setTimeout([6, 8].includes(config.bug) ? 180_000 : 120_000);
    await mkdir("../.cache/showcase/raw", { recursive: true });
    const browser = await playwright[browserName].launch({
      ...launchOptions,
      headless,
      channel,
    });
    try {
      const context = await browser.newContext({
        baseURL,
        viewport: { width: 1280, height: 720 },
        recordVideo: {
          dir: "../.cache/showcase/raw",
          size: { width: 1280, height: 720 },
        },
      });
      try {
        const page = await context.newPage();
        const proofTimes: number[] = [];
        const pack = await page.request.get(
          "/asset-pack/buildings/manifest.json",
        );
        expect(
          pack.ok(),
          "Imported DE building art is required for footage",
        ).toBe(true);
        const stoppedSite = [14, 9].includes(config.bug);
        const scenario =
          stoppedSite || config.bug === 6
            ? ""
            : config.bug === 12
              ? "?scenario=vassal-defense"
              : config.bug === 16
                ? "?scenario=imperial-army"
                : "?scenario=feudal-economy";
        expect(
          (await page.request.post(`/maps/reset${scenario}`)).status(),
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
        if (config.bug === 16) {
          await page.locator("#research-imperial-age").click();
          await expect(page.locator("#age")).toContainText("Imperial Age", {
            timeout: 30_000,
          });
        }
        const camera = async (x: number, y: number, zoom: number) => {
          const previous = Number(
            await page.locator("#scene").getAttribute("data-frame"),
          );
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
          await expect(page.locator("#minimap")).toHaveAttribute(
            "data-zoom",
            String(zoom),
          );
          await expect(page.locator("#scene")).toHaveAttribute(
            "data-chunks-failed",
            "0",
          );
          if (zoom >= 0.1)
            await expect
              .poll(
                async () =>
                  Number(
                    await page.locator("#scene").getAttribute("data-frame"),
                  ),
                { timeout: 30_000 },
              )
              .toBeGreaterThan(previous);
          await freshHud(page);
        };
        const contact = config.bug === 6 ? await contactProof(page, at) : null;
        if (contact) Object.assign(at, contact.corpse);
        if (config.bug === 8) {
          const actors = async () =>
            JSON.parse(
              (await page.locator("#scene").getAttribute("data-actors")) ??
                "[]",
            ) as { kind: string; x: number; y: number }[];
          await camera(at.x, at.y, 0.5);
          await page
            .locator("#train-villager")
            .click({ clickCount: 12, delay: 25 });
          await freshHud(page);
          const worker = (await actors()).find(
            (a) => a.kind === "Villager" && a.y < 480,
          );
          if (!worker) throw new Error("No starting villager");
          await page.mouse.click(worker.x, worker.y);
          await expect(page.locator("#selection-name")).toHaveText("Villager");
          await page.locator("#build-menu").click();
          for (const capacity of [10, 15, 20]) {
            await page.locator("#build-house").click();
            let site: [number, number] | undefined;
            const points: [number, number][] = [
              [1000, 360],
              [940, 360],
              [900, 360],
              [850, 280],
              [850, 400],
              [450, 400],
              [450, 280],
              [640, 470],
            ];
            for (const point of points) {
              await page.mouse.move(...point);
              await freshHud(page);
              if (
                (await page
                  .locator("#scene")
                  .getAttribute("data-placement-valid")) === "true"
              ) {
                site = point;
                break;
              }
            }
            if (!site) throw new Error("No legal housing site");
            await page.mouse.click(...site);
            await expect
              .poll(
                async () =>
                  Number(
                    (await page.locator("#population").textContent())?.split(
                      "/",
                    )[1],
                  ),
                { timeout: 20_000 },
              )
              .toBeGreaterThanOrEqual(capacity);
          }
          await page.locator("#build-menu").click();
          await expect
            .poll(
              async () =>
                (await actors()).filter((a) => a.kind === "Villager").length,
              { timeout: 45_000 },
            )
            .toBeGreaterThanOrEqual(15);
          await page.mouse.move(200, 85);
          await page.mouse.down();
          await page.mouse.move(1100, 505, { steps: 12 });
          await page.mouse.up();
          await page.locator("#garrison").click();
          const town = (await actors()).find((a) => a.kind === "TownCenter");
          if (!town) throw new Error("No Town Center");
          await page.mouse.click(town.x, town.y, { button: "right" });
          await centerVillage(page, true);
          await expect(page.locator("#garrison-count")).toHaveText("15 / 15", {
            timeout: 20_000,
          });
          await page
            .locator("#train-villager")
            .click({ clickCount: 15, delay: 25 });
          await expect(page.locator("#training-label")).toContainText(
            "Waiting for housing",
            { timeout: 20_000 },
          );
          await expect(
            page.locator("#training-queue .unit-token"),
          ).not.toHaveCount(0);
        }
        if (stoppedSite) {
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
          await expect(page.locator("#selection-name")).toContainText(
            "Villager",
          );
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
              (await page
                .locator("#scene")
                .getAttribute("data-placement-valid")) === "true"
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
        if (config.bug === 12) {
          at.x = 92;
          at.y = 92;
        }
        const zoom =
          config.bug === 6
            ? 2
            : config.bug === 18
              ? 0.1
              : stoppedSite
                ? 0.099
                : [8, 12].includes(config.bug)
                  ? 0.6
                  : 0.045;
        if (config.bug === 18) {
          at.x += 40;
          at.y += 40;
        }
        await camera(at.x, at.y, zoom);
        await expect(page.locator("#scene")).toHaveAttribute(
          "data-lod",
          zoom >= 0.1 ? "sprites" : "markers",
        );
        await freshHud(page);
        if (zoom < 0.1) {
          await expect
            .poll(() =>
              page.evaluate(() =>
                sessionStorage.getItem("aoeworld.cockpit-reveal"),
              ),
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
                  await page
                    .locator("#strategic-scene")
                    .getAttribute("data-sprites"),
                ),
              { timeout: 15_000 },
            )
            .toBeGreaterThan(0);
        }
        if (config.bug === 16 && config.side === "after")
          await expect(
            page
              .locator('#strategic-settlements [data-kind="TownCenter"]')
              .first(),
          ).toHaveAttribute("data-native", "142");
        if (config.bug === 14) {
          const labels = page.locator("#construction-labels");
          if (config.side === "before") await expect(labels).toBeVisible();
          else await expect(labels).toBeHidden();
        }
        await page.evaluate(({ bug, side }) => {
          const caption = document.createElement("div");
          const description =
            (
              {
                18: "inactive yellow domain circles",
                14: "unfinished buildings and world progress",
                9: "compact F1 hotkey labels",
                8: "villager queue and garrison portraits",
                6: "carcass ground contact; farm foundation",
                12: "Castle near corner",
                16: "far buildings follow the current age",
              } as Record<number, string>
            )[bug] ?? "native building art";
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
              layout: [
                "selection-content",
                "selection-heading",
                "selection-portrait",
                "selection-name",
                "selection-task",
                "selection-load",
                "selection-health",
                "building-panel",
                "garrison-panel",
                "garrison-units",
                "selected-icons",
              ].map((id) => {
                const n = document.getElementById(id);
                if (!n) return null;
                const css = getComputedStyle(n);
                return {
                  id,
                  box: n.getBoundingClientRect().toJSON(),
                  hidden: n.hidden,
                  display: css.display,
                  rows: css.gridTemplateRows,
                  font: css.font,
                  margin: css.margin,
                  padding: css.padding,
                };
              }),
              keycaps: Array.from(
                document.querySelectorAll(".dock-btn kbd"),
              ).map((n) => n.getBoundingClientRect().toJSON()),
              queue: Array.from(
                document.querySelectorAll("#training-queue .unit-token"),
              ).map((n) => n.getBoundingClientRect().toJSON()),
              garrison: Array.from(
                document.querySelectorAll("#garrison-units button"),
              ).map((n) => n.getBoundingClientRect().toJSON()),
              domains: document.getElementById("steward-domain")?.outerHTML,
              age: document.getElementById("age")?.textContent,
              construction: document.getElementById("construction-labels")
                ?.outerHTML,
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
        proofTimes.push(Date.now());
        // Intentional recorded screen time after all readiness checks.
        await page.waitForTimeout(2500);
        if (config.bug === 14)
          await expect
            .poll(() => page.locator("#construction-labels span").count())
            .toBeGreaterThan(0);
        if (contact) {
          await expect
            .poll(() => page.locator("#construction-labels span").count())
            .toBeGreaterThan(0);
          await camera(contact.foundation.x, contact.foundation.y, 1);
          await page.screenshot({
            path: `../.cache/showcase/bug-6-${config.side}-farm.png`,
          });
          proofTimes.push(Date.now());
          await page.waitForTimeout(2500);
        }
        proofTimes.push(Date.now());
        if (contact)
          await writeFile(
            `../.cache/showcase/bug-6-${config.side}-timings.json`,
            JSON.stringify(proofTimes),
          );
        const video = page.video();
        await page.close();
        if (!video) throw new Error("Recording was not created");
        await video.saveAs(
          `../.cache/showcase/bug-${config.bug}-${config.side}-raw.webm`,
        );
      } finally {
        await context.close();
      }
    } finally {
      await browser.close();
    }
  });
}
