import { fiveVillagerFixture } from "./fixtures/village.js";
import { activateSyntheticMap } from "../synthetic-map.js";
import { expect } from "@playwright/test";
import { isolatedTest as test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import {
  centerVillage,
  frameResource,
  scoutVillage,
} from "./fixtures/resources.js";

test("villager resource cursors, live resource selection, grid and own minimap markers", async ({
  page,
  request,
}) => {
  test.setTimeout(80_000);
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page, true);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected");
  const hash = process.env["AOE_E2E_LANDSCAPE_HASH"];
  if (!hash) throw new Error("Prepared landscape missing");
  const token = await page.evaluate(
    () => localStorage.getItem("aoeworld.resume-token") ?? "",
  );
  expect(
    (
      await request.post(`/maps/${hash}`, {
        headers: { "X-AoeWorld-Controller-Token": token },
      })
    ).ok(),
  ).toBeTruthy();
  await page.getByRole("button", { name: "Menu", exact: true }).click();
  await page.locator("#reconnect").click();
  await expect(page.locator("#minimap")).toHaveAttribute("data-map-hash", hash);
  await page.evaluate(() => {
    location.hash = "zoom=0.5";
  });
  await centerVillage(page, true);
  await scoutVillage(page, 5, 7);
  const actors = async () =>
    JSON.parse(
      (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
    ) as { kind: string; x: number; y: number }[];
  const villager = (await actors()).find((actor) => actor.kind === "Villager");
  if (!villager) throw new Error("Villager missing");
  await page.mouse.click(villager.x, villager.y);
  await expect(page.locator("#selection-name")).toHaveText("Villager");
  const tree = await frameResource(page, 1);
  await page.mouse.move(tree.x, tree.y);
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-command-cursor",
    "gather",
  );
  await expect(page.locator("#resource-highlight")).toBeHidden();
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-resource-hovered",
    "true",
  );
  await page.mouse.click(tree.x, tree.y, { button: "right" });
  await expect(page.locator("#workers-wood")).toHaveText("1", {
    timeout: 35_000,
  });
  await page.mouse.click(tree.x, tree.y);
  await expect(page.locator("#selection-name")).toHaveText("Tree");
  await expect(page.locator("#selection-load")).toContainText("remaining");
  const initial = await page.locator("#selection-load").innerText();
  // The top bar counts the woodcutter from the order on, while it still
  // walks to the tree, so the first chop may be further away.
  await expect(page.locator("#selection-load")).not.toHaveText(initial, {
    timeout: 35_000,
  });
  await expect(page.locator("#minimap-entities .minimap-unit")).toHaveCount(4);
  await expect(page.locator("#minimap-entities .minimap-building")).toHaveCount(
    1,
  );
  await centerVillage(page, true);
  const idle = (await actors())
    .filter((actor) => actor.kind === "Villager")
    .at(-1);
  if (!idle) throw new Error("Idle villager missing");
  await page.mouse.click(idle.x, idle.y);
  await expect(page.locator("#selection-name")).toHaveText("Villager");
  const gold = await frameResource(page, 2, 0.35, true);
  await page.mouse.click(gold.x, gold.y);
  await expect(page.locator("#selection-name")).toHaveText("Gold deposit");
  await expect(page.locator("#selection-load")).toContainText("gold remaining");
  await expect(
    page.locator('#minimap-resources circle[fill="#f2cf50"]').first(),
  ).toBeAttached();
  await page.locator("#grid").click();
  await expect(page.locator("#grid")).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press("g");
  await expect(page.locator("#grid")).toHaveAttribute("aria-pressed", "false");
});

test("villager cursors use composed passability for grass, water, cliffs and buildings", async ({
  page,
}) => {
  await gameAssets(page);
  await fiveVillagerFixture(page);
  await page.goto("/?play=1");
  await expect(page.locator("#population")).toHaveText("6/10");
  await activateSyntheticMap(page);
  await page.evaluate(() => {
    location.hash = "x=8188.5&y=8196.5&zoom=.5";
  });
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-camera-x",
    "8188.5",
  );
  await page.mouse.click(640, 360);
  await expect(page.locator("#selection-name")).toHaveText("Villager");
  // Composed fixture tiles: water at (8195,8188), cliff at (8188,8188),
  // ordinary grass outside the Town Center's authoritative footprint.
  for (const [x, y, cursor] of [
    [8187, 8196, "move"],
    [8195, 8188, "forbidden"],
    [8188, 8188, "forbidden"],
    // Villagers garrison in their own Town Center, as in AoE2.
    [8192, 8192, "garrison"],
  ] as const) {
    await page.evaluate(
      ({ x, y }) => {
        location.hash = `x=${x + 0.5}&y=${y + 0.5}&zoom=.5`;
      },
      { x, y },
    );
    await expect(page.locator("#minimap")).toHaveAttribute(
      "data-camera-x",
      String(x + 0.5),
    );
    await page.mouse.move(640, 360);
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-command-cursor",
      cursor,
    );
  }
});
