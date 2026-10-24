import { expect, type Page } from "@playwright/test";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { activatePackage } from "../../start/package.js";
import {
  centerVillage,
  frameResource,
  freshHud,
} from "../fixtures/resources.js";

type Tile = { x: number; y: number };
type Actor = { id: number; kind: string; x: number; y: number };

async function actors(page: Page): Promise<Actor[]> {
  return JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as Actor[];
}

async function frame(page: Page, tile: Tile, zoom = 0.55) {
  await page.evaluate(
    ({ x, y, zoom }) => {
      location.hash = `x=${x}&y=${y}&zoom=${zoom}`;
    },
    { ...tile, zoom },
  );
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-chunks-pending",
    "0",
    { timeout: 30_000 },
  );
  await expect
    .poll(async () =>
      Number(await page.locator("#minimap").getAttribute("data-camera-x")),
    )
    .toBeCloseTo(tile.x, 1);
  await expect
    .poll(async () =>
      Number(await page.locator("#minimap").getAttribute("data-zoom")),
    )
    .toBeCloseTo(zoom, 3);
  await freshHud(page);
}

async function point(page: Page, tile: Tile) {
  return page.locator("#scene").evaluate((canvas, tile) => {
    const map = document.getElementById("minimap");
    const cameraX = Number(map?.getAttribute("data-camera-x"));
    const cameraY = Number(map?.getAttribute("data-camera-y"));
    const zoom = Number(map?.getAttribute("data-zoom"));
    const element = canvas as HTMLCanvasElement;
    const bounds = element.getBoundingClientRect();
    const dx = tile.x + 0.5 - cameraX,
      dy = tile.y + 0.5 - cameraY;
    return {
      x:
        bounds.left +
        bounds.width / 2 +
        ((dx - dy) * 64 * zoom * bounds.width) / element.width,
      y:
        bounds.top +
        bounds.height / 2 +
        ((dx + dy) * 32 * zoom * bounds.height) / element.height,
    };
  }, tile);
}

/** Reveal fish through the selected observer's real LOS before gathering. */
async function revealFish(page: Page, tile: Tile, naval: boolean) {
  await frame(page, tile, 0.75);
  for (const [dx, dy] of [
    [-1, 0],
    [0, -1],
    [0, 1],
    [1, 0],
    [-2, 0],
    [-3, 0],
  ] as const) {
    const target = { x: tile.x + dx, y: tile.y + dy };
    const at = await point(page, target);
    await page.mouse.move(at.x, at.y);
    await expect(page.locator("#tile-inspection")).toContainText(
      `tile ${target.x}, ${target.y}:`,
    );
    await freshHud(page);
    const eligible = naval
      ? (await page.locator("#scene").getAttribute("data-command-cursor")) ===
        "move"
      : (await page.locator("#tile-inspection").textContent())?.endsWith(
          " · passable",
        );
    if (!eligible) continue;
    await page.mouse.click(at.x, at.y, { button: "right" });
    await expect(page.locator("#unit-state")).not.toHaveText("unit idle");
    await expect(page.locator("#unit-state")).toHaveText("unit idle", {
      timeout: 60_000,
    });
    return;
  }
  throw new Error("No reachable approach beside the fish");
}

test("coastal village builds a Dock, trains a Fishing Ship, and deposits fish food", async ({
  page,
  request,
}) => {
  test.setTimeout(420_000);
  const scene = JSON.parse(
    await readFile("../.cache/fishing/scene.json", "utf8"),
  ) as { hash: string; dock: Tile; shore: Tile; deep: Tile; start: Tile };
  expect((await request.post("/maps/reset")).status()).toBe(204);
  const art = await gameAssets(page);
  const shot = async (name: string) => {
    await mkdir("../.cache/fish-shots", { recursive: true });
    const prefix = art === "local AoE II pack" ? "native-" : "";
    await page.screenshot({
      path: `../.cache/fish-shots/${prefix}${name}.png`,
    });
  };
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  expect((await activatePackage(page, scene.hash)).start_available).toBe(true);
  await page.reload();
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-map-hash",
    scene.hash,
    { timeout: 30_000 },
  );
  await centerVillage(page);
  const scout = (await actors(page)).find((a) => a.kind === "Scout");
  if (!scout) throw new Error("Scout missing");
  await page.mouse.click(scout.x, scout.y);
  await expect(page.locator("#selection-name")).toHaveText("Scout Cavalry");
  await revealFish(page, scene.shore, false);
  await frame(page, scene.shore, 1.0);
  await shot("shore-fish");
  const fish = await point(page, scene.shore);
  await page.mouse.click(fish.x, fish.y);
  await expect(page.locator("#selection")).toHaveAttribute(
    "data-kind",
    "Resource",
  );
  await expect(page.locator("#selection-name")).toHaveText("Shore fish");
  await expect(page.locator("#unit-health-bars .unit-hp-bar")).toHaveCount(0);
  await page.keyboard.press("Escape");
  await centerVillage(page);
  const workers = (await actors(page)).filter((a) => a.kind === "Villager");
  const woodcutter = workers[0],
    builder = workers[1],
    fisher = workers[2];
  if (!woodcutter || !builder || !fisher)
    throw new Error("Three villagers required");
  await page.mouse.click(woodcutter.x, woodcutter.y);
  const wood = await frameResource(page, 1);
  await page.mouse.click(wood.x, wood.y, { button: "right" });
  await expect
    .poll(
      async () => Number(await page.locator("#stockpile-wood").textContent()),
      { timeout: 150_000 },
    )
    .toBeGreaterThanOrEqual(230);
  await centerVillage(page);
  const buildActor = (await actors(page)).find((a) => a.id === builder.id);
  if (!buildActor) throw new Error("Builder missing");
  await page.mouse.click(buildActor.x, buildActor.y);
  await page.locator("#build-menu").click();
  await expect(page.locator("#build-dock")).toContainText("150 wood");
  const dockButton = page.locator("#build-dock");
  const buttonBounds = await dockButton.boundingBox();
  if (!buttonBounds) throw new Error("Dock button missing");
  expect(buttonBounds.y + buttonBounds.height).toBeLessThanOrEqual(704);
  const camera = () =>
    page
      .locator("#minimap")
      .evaluate((node) => [
        node.getAttribute("data-camera-x"),
        node.getAttribute("data-camera-y"),
      ]);
  const before = await camera();
  await dockButton.hover();
  await page.waitForTimeout(300);
  expect(await camera()).toEqual(before);
  await page.locator("#build-dock").click();
  await frame(page, scene.dock);
  const site = await point(page, scene.dock);
  await page.mouse.move(site.x, site.y);
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-placement-valid",
    "true",
  );
  await shot("dock-preview");
  await page.mouse.click(site.x, site.y);
  await expect
    .poll(async () => (await actors(page)).some((a) => a.kind === "Dock"))
    .toBe(true);
  const dock = (await actors(page)).find((a) => a.kind === "Dock");
  if (!dock) throw new Error("Dock missing");
  await page.mouse.click(dock.x, dock.y);
  await expect(page.locator("#selection-name")).toHaveText("Dock");
  await shot("dock-building");
  await expect(page.locator("#building-status")).toContainText("Complete", {
    timeout: 100_000,
  });
  if (art === "local AoE II pack") {
    await frame(
      page,
      {
        x: (scene.start.x + scene.dock.x + 1.5) / 2,
        y: (scene.start.y + scene.dock.y + 1.5) / 2,
      },
      0.45,
    );
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-village-art",
      "native",
    );
    const visible = await actors(page);
    for (const kind of ["TownCenter", "Dock"]) {
      expect(
        visible.some(
          (a) =>
            a.kind === kind && a.x > 0 && a.x < 1280 && a.y > 0 && a.y < 600,
        ),
      ).toBe(true);
    }
    const served = await request.get("/pkg/aoe_client_bg.wasm");
    expect(served.ok()).toBe(true);
    const sha256 = (bytes: Buffer) =>
      createHash("sha256").update(bytes).digest("hex");
    const bundleSha256 = sha256(await served.body());
    expect(bundleSha256).toBe(
      sha256(await readFile("../web/pkg/aoe_client_bg.wasm")),
    );
    await mkdir("../.cache/tc-shots", { recursive: true });
    const screenshot = await page.screenshot({
      path: "../.cache/tc-shots/final-dock.png",
    });
    await writeFile(
      "../.cache/tc-shots/final-dock.json",
      JSON.stringify(
        {
          health: await (await request.get("/health")).json(),
          bundleSha256,
          screenshotSha256: sha256(screenshot),
          scenario: scene,
          actors: visible,
          renderer: await page
            .locator("#playground")
            .getAttribute("data-renderer"),
          camera: await page
            .locator("#minimap")
            .evaluate((map) =>
              ["data-camera-x", "data-camera-y", "data-zoom"].map((key) =>
                map.getAttribute(key),
              ),
            ),
        },
        null,
        2,
      ),
    );
    await frame(page, scene.dock);
  }
  await expect(page.locator("#train-villager")).toHaveAttribute(
    "aria-label",
    /Fishing Ship/,
  );
  await page.locator("#train-villager").click();
  await expect(page.locator("#training-label")).toContainText("Fishing Ship");
  await shot("dock-training");
  await expect
    .poll(
      async () => (await actors(page)).some((a) => a.kind === "FishingShip"),
      { timeout: 60_000 },
    )
    .toBe(true);
  const ship = (await actors(page)).find((a) => a.kind === "FishingShip");
  if (!ship) throw new Error("Fishing Ship missing");
  await page.mouse.click(ship.x, ship.y);
  await expect(page.locator("#selection-name")).toHaveText("Fishing Ship");
  await expect(page.locator("#unit-health-bars .unit-hp-bar")).toHaveCount(1);
  await revealFish(page, scene.deep, true);
  await shot("deep-fish");
  await frame(page, scene.deep, 0.75);
  const deep = await point(page, scene.deep);
  await page.mouse.click(deep.x, deep.y, { button: "right" });
  await expect(page.locator("#selection-task")).toHaveText("Fishing", {
    timeout: 60_000,
  });
  await expect(page.locator("#command-hint")).toHaveText(
    "Right-click: fish or move",
  );
  await expect(page.locator("#selection-load")).toContainText("/15 food");
  await shot("deep-fishing");
  const initial = Number(await page.locator("#stockpile-food").textContent());
  await expect
    .poll(
      async () => Number(await page.locator("#stockpile-food").textContent()),
      { timeout: 100_000 },
    )
    .toBeGreaterThanOrEqual(initial + 15);
  await frame(page, scene.dock);
  await shot("ship-return");
  await centerVillage(page);
  const shoreFisher = (await actors(page)).find((a) => a.id === fisher.id);
  if (!shoreFisher) throw new Error("Fisher missing");
  await page.mouse.click(shoreFisher.x, shoreFisher.y);
  await frame(page, scene.shore);
  const shore = await point(page, scene.shore);
  // Fish show the fishing cursor, never the builder's hammer-like default.
  await page.mouse.move(shore.x, shore.y);
  await freshHud(page);
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-command-cursor",
    "gather",
  );
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-gather-kind",
    "fish",
  );
  await page.mouse.click(shore.x, shore.y, { button: "right" });
  await expect(page.locator("#selection-task")).toHaveText("Fishing", {
    timeout: 100_000,
  });
  await shot("shore-fisher");
});
