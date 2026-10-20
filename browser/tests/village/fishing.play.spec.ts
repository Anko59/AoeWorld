import { expect, type Page } from "@playwright/test";
import { mkdir, readFile } from "node:fs/promises";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { activatePackage } from "../start/package.js";
import { centerVillage, frameResource } from "./resources.js";

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

test("coastal village builds a Dock, trains a Fishing Ship, and deposits fish food", async ({
  page,
  request,
}) => {
  test.setTimeout(420_000);
  const scene = JSON.parse(
    await readFile("../.cache/fishing/scene.json", "utf8"),
  ) as { hash: string; dock: Tile; shore: Tile; deep: Tile };
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
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
  await page.locator("#build-dock").click();
  await frame(page, scene.dock);
  const site = await point(page, scene.dock);
  await page.mouse.move(site.x, site.y);
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-placement-valid",
    "true",
  );
  await mkdir("../.cache/fish-shots", { recursive: true });
  await page.screenshot({ path: "../.cache/fish-shots/dock-preview.png" });
  await page.mouse.click(site.x, site.y);
  await expect
    .poll(async () => (await actors(page)).some((a) => a.kind === "Dock"))
    .toBe(true);
  const dock = (await actors(page)).find((a) => a.kind === "Dock");
  if (!dock) throw new Error("Dock missing");
  await page.mouse.click(dock.x, dock.y);
  await expect(page.locator("#selection-name")).toHaveText("Dock");
  await page.screenshot({ path: "../.cache/fish-shots/dock-building.png" });
  await expect(page.locator("#building-status")).toContainText("Complete", {
    timeout: 100_000,
  });
  await expect(page.locator("#train-villager")).toHaveAttribute(
    "aria-label",
    /Fishing Ship/,
  );
  await page.locator("#train-villager").click();
  await expect(page.locator("#training-label")).toContainText("Fishing Ship");
  await page.screenshot({ path: "../.cache/fish-shots/dock-training.png" });
  await expect
    .poll(
      async () => (await actors(page)).some((a) => a.kind === "FishingShip"),
      { timeout: 60_000 },
    )
    .toBe(true);
  const ship = (await actors(page)).find((a) => a.kind === "FishingShip");
  if (!ship) throw new Error("Fishing Ship missing");
  await page.mouse.click(ship.x, ship.y);
  await frame(page, scene.deep, 0.75);
  const deep = await point(page, scene.deep);
  await page.mouse.click(deep.x, deep.y, { button: "right" });
  await expect(page.locator("#selection-task")).toHaveText("Fishing", {
    timeout: 60_000,
  });
  await page.screenshot({ path: "../.cache/fish-shots/deep-fishing.png" });
  const initial = Number(await page.locator("#stockpile-food").textContent());
  await expect
    .poll(
      async () => Number(await page.locator("#stockpile-food").textContent()),
      { timeout: 100_000 },
    )
    .toBeGreaterThanOrEqual(initial + 15);
  await frame(page, scene.dock);
  await page.screenshot({ path: "../.cache/fish-shots/ship-return.png" });
  await centerVillage(page);
  const shoreFisher = (await actors(page)).find((a) => a.id === fisher.id);
  if (!shoreFisher) throw new Error("Fisher missing");
  await page.mouse.click(shoreFisher.x, shoreFisher.y);
  await frame(page, scene.shore);
  const shore = await point(page, scene.shore);
  await page.mouse.click(shore.x, shore.y, { button: "right" });
  await expect(page.locator("#selection-task")).toHaveText("Fishing", {
    timeout: 100_000,
  });
  await page.screenshot({ path: "../.cache/fish-shots/shore-fisher.png" });
});
