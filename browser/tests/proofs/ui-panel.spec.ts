import { expect } from "@playwright/test";
import { readFile, writeFile } from "node:fs/promises";
import { test } from "../../server.js";
import { forceWebGl } from "../rendering/backends.js";
import { bundleProof } from "./bundle.js";
import { castleProof } from "./castle.js";
import { generalRiverProof, type RiverRecording } from "./general-river.js";
import { generalProof } from "./general.js";
import { generalAppearanceProof } from "./general-appearance.js";
import { siegeProof } from "./siege.js";
import { regressFireProof } from "./regress/regress-fire.js";
import { regressPickingProof } from "./regress/regress-picking.js";
import { regressFarmCursorProof } from "./regress/regress-farm-cursor.js";
import { pressureProof } from "./pressure.js";
import { townCenterProof } from "./town-center.js";
import { imperialArtProof } from "./imperial/art.js";
import { startupProof } from "./imperial/startup.js";
import { skirmishStartupProof } from "./imperial/skirmish-startup.js";
import { centerVillage } from "../village/fixtures/resources.js";

test.use({ video: { mode: "on", size: { width: 1280, height: 720 } } });

test("local original panel proof with real selection and training", async ({
  page,
}) => {
  const config = await readFile("../.cache/ui/proof.json", "utf8").then(
    (text) =>
      JSON.parse(text) as {
        phase: string;
        river?: RiverRecording;
        url: string;
        renderer?: "webgpu" | "webgl2";
        arenaToken?: string;
        side?: "before" | "after";
        agesOnly?: boolean;
      },
    () => null,
  );
  test.skip(!config, "Local panel proof was not requested");
  if (!config) return;
  test.setTimeout(
    config.phase.startsWith("imperial-art") ||
      config.phase === "imperial-load-compare"
      ? 600_000
      : ["castle", "de-town-center"].includes(config.phase)
        ? 300_000
        : 180_000,
  );
  const url = new URL(config.url);
  expect(url.hostname).toBe("127.0.0.1");
  expect(url.protocol).toBe("http:");
  expect(["8080", "8081", "8097"]).not.toContain(url.port);
  const health = await page.request.get(`${config.url}health`);
  expect(health.ok()).toBe(true);
  if (config.phase.startsWith("skirmish-startup-")) {
    test.setTimeout(600_000);
    await skirmishStartupProof(page, config.url, config.phase.slice(17));
    return;
  }
  if (config.phase === "imperial-load-compare") {
    const browser = page.context().browser();
    if (!browser) throw new Error("A fresh browser context is required");
    for (const label of [
      "trial-1",
      "de-1",
      "trial-2",
      "de-2",
      "trial-3",
      "de-3",
    ]) {
      const context = await browser.newContext();
      try {
        await startupProof(await context.newPage(), config.url, label);
      } finally {
        await context.close();
      }
    }
    return;
  }
  if (config.phase.startsWith("imperial-load-")) {
    await startupProof(page, config.url, config.phase.slice(14));
    return;
  }
  if (config.phase.startsWith("imperial-art")) {
    await page.setViewportSize({ width: 1280, height: 720 });
    await imperialArtProof(page, config.url, config.phase.endsWith("startup"));
    return;
  }
  if (config.phase.startsWith("pressure-")) {
    await pressureProof(page, config.url, config.phase);
    return;
  }
  await page.setViewportSize({ width: 1280, height: 720 });
  if (config.renderer !== "webgpu") await forceWebGl(page);
  if (config.phase === "general-river") {
    if (!config.river) throw new Error("River recording setup missing");
    await generalRiverProof(page, config.url, config.river);
    return;
  }
  if (config.phase === "de-town-center") {
    if (!config.arenaToken) throw new Error("Local arena token required");
    await townCenterProof(
      page,
      config.url,
      config.arenaToken,
      config.side ?? "after",
      config.agesOnly,
    );
    return;
  }
  if (config.phase === "regress-farm-cursor") {
    await regressFarmCursorProof(page, config.url);
    return;
  }
  if (config.phase === "regress-picking") {
    await regressPickingProof(page, config.url);
    return;
  }
  if (config.phase === "regress-fire") {
    await regressFireProof(page, config.url);
    return;
  }
  if (config.phase === "general") {
    await generalProof(page, config.url, config.renderer ?? "canvas");
    return;
  }
  if (config.phase === "general-appearance") {
    await generalAppearanceProof(page, config.url);
    return;
  }
  if (config.phase === "siege") {
    await siegeProof(page, config.url);
    return;
  }
  if (config.phase.startsWith("bundle")) {
    await bundleProof(page, config.url, config.phase === "bundle-film");
    return;
  }
  if (config.phase.startsWith("castle")) {
    await castleProof(
      page,
      config.url,
      config.renderer ?? "webgl2",
      config.phase.startsWith("castle-startup"),
      config.phase === "castle-continue" || config.phase.endsWith("-existing"),
      config.phase === "castle-tc",
    );
    return;
  }
  expect((await page.request.post(`${config.url}maps/reset`)).status()).toBe(
    204,
  );
  await page.goto(`${config.url}?play=1`);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await centerVillage(page, true);
  const actors = async () =>
    JSON.parse(
      (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
    ) as { kind: string; x: number; y: number }[];
  const select = async (kind: string) => {
    const a = (await actors()).find((a) => a.kind === kind);
    if (!a) throw new Error(`${kind} missing`);
    await page.mouse.click(a.x, a.y);
    await expect(page.locator("#selection")).toHaveAttribute("data-kind", kind);
  };
  const capture = async (name: string) => {
    await page.screenshot({ path: `../.cache/ui/${config.phase}-${name}.png` });
    await writeFile(
      `../.cache/ui/${config.phase}-${name}.json`,
      JSON.stringify(
        {
          health: await health.json(),
          actors: await actors(),
          selection: await page.locator("#selection").innerText(),
          interfaceArt: await page
            .locator("html")
            .getAttribute("data-interface-art"),
        },
        null,
        2,
      ),
    );
  };
  await capture("town-center");
  if (config.phase === "dark") return;
  await select("Villager");
  await capture("villager");
  const villager = (await actors()).find((a) => a.kind === "Villager");
  if (!villager) throw new Error("Villager missing");
  await page.mouse.dblclick(villager.x, villager.y);
  await expect(page.locator("#selected-icons .unit-token")).toHaveCount(3);
  await capture("multi-selection");
  await select("Villager");
  await page.locator("#build-military-menu").click();
  await page.locator("#build-barracks").click();
  let placed = false;
  for (const [x, y] of [
    [420, 320],
    [850, 320],
    [420, 420],
    [850, 420],
    [640, 220],
  ]) {
    if (x === undefined || y === undefined) continue;
    await page.mouse.move(x, y);
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-order-mode",
      "build",
    );
    await page.waitForTimeout(350);
    if (
      (await page.locator("#scene").getAttribute("data-placement-valid")) ===
      "true"
    ) {
      await page.mouse.click(x, y);
      placed = true;
      break;
    }
  }
  expect(placed).toBe(true);
  await expect
    .poll(async () => (await actors()).some((a) => a.kind === "Barracks"))
    .toBe(true);
  await select("Barracks");
  await expect(page.locator("#building-status")).toHaveText("Complete", {
    timeout: 85_000,
  });
  await page.locator("#train-villager").click();
  await expect(page.locator("#training-queue .unit-token")).toHaveCount(1);
  await capture("barracks-queue");
});
