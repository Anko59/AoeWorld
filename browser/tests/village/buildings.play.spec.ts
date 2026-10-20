import { expect } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { centerVillage } from "./resources.js";

test("villager build menu places a House and watches authoritative completion", async ({
  page,
  request,
}) => {
  test.setTimeout(210_000);
  expect((await request.post("/maps/reset")).status()).toBe(204);
  const assets = await gameAssets(page);
  // A supported trial pack must exercise the full flow even without DE art.
  await page.route("**/asset-pack/buildings/**", (route) =>
    route.fulfill({
      status: 404,
      body: "Trial pack without optional DE overlay",
    }),
  );
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await centerVillage(page);
  const actors = async () =>
    JSON.parse(
      (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
    ) as { kind: string; x: number; y: number }[];
  await mkdir("../.cache/client-shots", { recursive: true });
  await writeFile(
    "../.cache/client-shots/start-actors.json",
    JSON.stringify(await actors()),
  );
  expect(
    await page.evaluate(async () => {
      const game = await import(
        new URL("/pkg/aoe_client.js", location.href).href
      );
      return "replay_hash" in game;
    }),
  ).toBe(false);
  const worker = (await actors()).find((a) => a.kind === "Villager");
  if (!worker) throw new Error("Starting villager missing");
  const selectWorker = async (nth: number, boxed = false) => {
    const worker = (await actors()).filter((a) => a.kind === "Villager")[nth];
    if (!worker) throw new Error("Builder missing");
    if (boxed) {
      // A stationary farmer can share ground with a foreground sheep.
      await page.mouse.move(worker.x - 6, worker.y - 6);
      await page.mouse.down();
      await page.mouse.move(worker.x + 6, worker.y + 6, { steps: 3 });
      await page.mouse.up();
    } else {
      await page.mouse.click(worker.x, worker.y);
    }
    await expect(page.locator("#selection-name")).toHaveText("Villager");
  };
  await page.mouse.click(worker.x, worker.y);
  await page.locator("#build-menu").click();
  await expect(page.locator("#build-farm")).toBeDisabled();
  await expect(page.locator("#build-house")).toContainText("25");
  await mkdir("../.cache/client-shots", { recursive: true });
  const frameCount = Number(
    await page.locator("#playground").getAttribute("data-atlas-frames"),
  );
  expect(frameCount).toBeGreaterThan(0);
  expect(frameCount).toBeLessThanOrEqual(5120);
  await writeFile(
    "../.cache/client-shots/atlas.json",
    JSON.stringify({ assets, frameCount, pages: 4, atlasBytes: 67108864 }),
  );
  await page.screenshot({ path: "../.cache/client-shots/build-menu.png" });
  await page.locator("#build-house").click();
  await page.keyboard.press("Escape");
  await expect(page.locator("#scene")).toHaveAttribute("data-order-mode", "");
  await expect(page.locator("#stockpile-wood")).toHaveText("200");
  await page.locator("#build-menu").click();
  await page.locator("#build-house").click();
  await page.mouse.move(640, 360);
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-placement-valid",
    "false",
  );
  await page.mouse.click(640, 360);
  await expect(page.locator("#stockpile-wood")).toHaveText("200");
  let placed = false;
  for (const [x, y] of [
    [420, 320],
    [850, 320],
    [420, 420],
    [850, 420],
    [640, 220],
  ] as const) {
    await page.mouse.move(x, y);
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-order-mode",
      "build",
    );
    await page.waitForTimeout(300);
    if (
      (await page.locator("#scene").getAttribute("data-placement-valid")) ===
      "true"
    ) {
      await page.screenshot({
        path: "../.cache/client-shots/placement-preview.png",
      });
      await page.mouse.click(x, y);
      placed = true;
      break;
    }
  }
  expect(placed).toBe(true);
  await expect(page.locator("#stockpile-wood")).toHaveText("175");
  await expect
    .poll(async () => (await actors()).some((a) => a.kind === "House"))
    .toBe(true);
  const house = (await actors()).find((a) => a.kind === "House");
  if (!house) throw new Error("House foundation missing");
  await page.mouse.click(house.x, house.y);
  await expect(page.locator("#selection-name")).toHaveText("House");
  await expect(page.locator("#building-status")).toContainText("Constructing");
  await selectWorker(0);
  await page.locator("#stop-units").click();
  await expect(page.locator("#selection-task")).toHaveText("Idle");
  await page.mouse.click(house.x, house.y, { button: "right" });
  await expect(page.locator("#selection-task")).toHaveText("Building");
  await page.mouse.click(house.x, house.y);
  await expect(page.locator("#building-panel")).toBeVisible();
  await expect(page.locator("#selection-name")).toHaveText("House");
  await expect(page.locator("#building-status")).toContainText("Constructing");
  await expect(page.locator("#building-status")).toContainText("Complete", {
    timeout: 60_000,
  });
  await expect(page.locator("#population")).toHaveText("4/10");
  await expect(page.locator("#building-hp")).toHaveText("550 / 550 HP");
  // The same UI path exercises the Mill prerequisite, farm work and drag walls.
  for (const [id, kind, x, y] of [
    ["mill", "Mill", 900, 380],
    ["farm", "Farm", 300, 200],
  ] as const) {
    await selectWorker(1);
    await page.locator("#build-menu").click();
    await page.locator(`#build-${id}`).click();
    await page.mouse.move(x, y);
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-placement-valid",
      "true",
    );
    await page.mouse.click(x, y);
    await expect
      .poll(async () => (await actors()).some((a) => a.kind === kind))
      .toBe(true);
    const foundation = (await actors()).find((a) => a.kind === kind);
    if (!foundation) throw new Error(`${kind} missing`);
    // Wildlife now occupies the map: this Farm's center has a sheep on it.
    // Select its unobstructed ground, rather than the foreground animal.
    await page.mouse.click(
      foundation.x,
      foundation.y + (kind === "Farm" ? 32 : 0),
    );
    await expect(page.locator("#selection-name")).toHaveText(kind);
    await expect(page.locator("#building-status")).toContainText("Complete", {
      timeout: 70_000,
    });
  }
  await expect(page.locator("#workers-food")).toHaveText("1", {
    timeout: 15_000,
  });
  await selectWorker(1, true);
  await page.locator("#stop-units").click();
  await expect(page.locator("#workers-food")).toHaveText("0");
  const farm = (await actors()).find((a) => a.kind === "Farm");
  if (!farm) throw new Error("Completed Farm missing");
  await page.mouse.click(farm.x, farm.y + 32, { button: "right" });
  await expect(page.locator("#selection-task")).toHaveText("Farming");
  await expect(page.locator("#workers-food")).toHaveText("1");
  await selectWorker(0);
  await page.locator("#build-menu").click();
  await page.locator("#build-palisade-wall").click();
  await page.mouse.move(1000, 210);
  await page.mouse.down();
  await page.mouse.move(1120, 150, { steps: 8 });
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-placement-valid",
    "true",
  );
  await page.mouse.up();
  await expect
    .poll(
      async () =>
        (await actors()).filter((a) => a.kind === "PalisadeWall").length,
    )
    .toBeGreaterThan(1);
  await expect(page.locator("#construction-labels span")).toHaveCount(0, {
    timeout: 60_000,
  });
  await centerVillage(page, true);
  await writeFile(
    "../.cache/client-shots/village-actors.json",
    JSON.stringify(await actors()),
  );
  await page.screenshot({
    path: "../.cache/client-shots/constructed-village.png",
  });
  await page.locator("#research-loom").click();
  await expect(page.locator("#research-status")).toContainText("Loom");
  await expect(page.locator("#stockpile-gold")).toHaveText("50");
  await page.locator("#cancel-research").click();
  await expect(page.locator("#research-status")).toHaveText("");
  await page.keyboard.press("q");
  await expect(page.locator("#training-label")).toContainText("Villager");
  await page.locator("#cancel-training").click();
  await expect(page.locator("#training")).toBeHidden();
});
