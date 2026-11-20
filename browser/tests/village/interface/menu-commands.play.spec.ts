import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { fiveVillagerFixture } from "../fixtures/village.js";
import { monitorCommands } from "../fixtures/menu-shortcuts.js";
import { freshHud } from "../fixtures/resources.js";
import {
  building,
  inputFixture,
  screen,
  selectWorker,
} from "../input/fixture.js";

const unitClasses = [
  [0, "Villager"],
  [1, "Scout Cavalry"],
  [2, "Militia"],
  [4, "Archer"],
  [5, "Skirmisher"],
  [6, "Spearman"],
  [12, "Knight"],
  [14, "Monk"],
  [15, "Mangonel"],
  [16, "Battering Ram"],
  [22, "Vassal"],
  [23, "General"],
  [34, "Steward"],
  [35, "Trade Cart"],
  [36, "Trade Cog"],
  [3, "Fishing Ship"],
  [20, "Transport Ship"],
  [21, "War Galley"],
  [17, "Galley"],
  [18, "Fire Ship"],
  [19, "Demolition Ship"],
  [48, "Cavalry Archer"],
  [50, "Camel Rider"],
  [52, "Scorpion"],
  [54, "Hand Cannoneer"],
  [56, "Trebuchet"],
  [57, "Trebuchet (unpacked)"],
  [58, "Petard"],
] as const;

test("unit classes select their own card and route displayed keyboard orders", async ({
  page,
}) => {
  test.setTimeout(180_000);
  const ownedUnits = unitClasses.map(([kind, name], index) => ({
    id: index + 100,
    x: 8173.5 + 5 * (index % 7),
    y: 8173.5 + 5 * Math.floor(index / 7),
    kind,
    name,
  }));
  await inputFixture(page, "webgl2", [], false, 0, undefined, { ownedUnits });
  const scene = page.locator("#scene");
  const checkShortcuts = await monitorCommands(page);
  for (const { id, x, y, name } of ownedUnits) {
    await page.evaluate(
      ([x, y]) => {
        location.hash = `x=${x}&y=${y}&zoom=.8`;
      },
      [x, y],
    );
    await expect(page.locator("#minimap")).toHaveAttribute(
      "data-camera-x",
      String(x),
    );
    await freshHud(page);
    const target = await scene.evaluate((node, id) => {
      const actors = JSON.parse(node.getAttribute("data-actors") ?? "[]") as {
        id: number;
        x: number;
        y: number;
      }[];
      return actors.find((actor) => actor.id === id);
    }, id);
    if (!target) throw new Error(`Unit missing: ${name}`);
    await page.mouse.click(target.x, target.y);
    await expect(page.locator("#selection-name")).toHaveText(name);
    await expect(page.locator("#selection")).toHaveAttribute(
      "data-type",
      "unit",
    );
    const move = page.locator("#move-units");
    await expect(move).toHaveAttribute("aria-keyshortcuts", "Q");
    await scene.focus();
    await page.keyboard.press("q");
    await expect(scene).toHaveAttribute("data-order-mode", "move");
    await page.keyboard.press("g");
    await expect(scene).toHaveAttribute("data-order-mode", "");
    await checkShortcuts(name);
    if (name === "Villager") {
      for (const [key, group] of [
        ["b", "Economic buildings"],
        ["v", "Military buildings"],
      ] as const) {
        await scene.focus();
        await page.keyboard.press(key);
        await expect(page.locator("#command-context")).toHaveText(group);
        await checkShortcuts(group);
        await scene.focus();
        await page.keyboard.press("Escape");
        await expect(page.locator("#command-context")).toHaveText("Villager");
      }
    }
    await scene.focus();
    await page.keyboard.press("Escape");
  }
});

for (const [kind, name, wire] of [
  ["LightCavalry", "Light Cavalry", 13],
  ["Galleon", "Galleon", 41],
  ["BombardCannon", "Bombard Cannon", 55],
] as const) {
  test(`${name} selects the right portrait and exposes combat orders`, async ({
    page,
  }) => {
    test.setTimeout(90_000);
    await gameAssets(page);
    await fiveVillagerFixture(page, undefined, () => 0, false, true, wire);
    await page.goto("/?play=1");
    await expect(page.locator("#connection")).toHaveText("connected");
    const scene = page.locator("#scene");
    await expect(scene).toHaveAttribute("data-actors", /"owned":true/);
    const actor = await scene.evaluate((element) => {
      const actors = JSON.parse(
        element.getAttribute("data-actors") ?? "[]",
      ) as { id: number; x: number; y: number }[];
      return actors.find(({ id }) => id === 2);
    });
    if (!actor) throw new Error("Unit missing");
    await page.mouse.click(actor.x, actor.y);
    await expect(page.locator("#selection")).toHaveAttribute("data-kind", kind);
    await expect(page.locator("#selection-name")).toHaveText(name);
    await expect(
      page.locator("#selection-portrait .art-icon"),
    ).not.toHaveAttribute("data-ui-id", "-1");
    await expect(page.locator("#attack-move-units")).toBeEnabled();
    await page.keyboard.press("r");
    await expect(scene).toHaveAttribute("data-order-mode", "attack-move");
    await page.keyboard.press("g");
    await expect(scene).toHaveAttribute("data-order-mode", "");

    if (kind === "LightCavalry") {
      await page.mouse.dblclick(actor.x, actor.y);
      await expect(page.locator("#selection-name")).toHaveText(
        "5 Light Cavalry selected",
      );
      const checkShortcuts = await monitorCommands(page);
      await checkShortcuts("Light Cavalry group");
    }
  });
}

test("mixed selection uses unit commands and foreign and wildlife cards clear them", async ({
  page,
}) => {
  test.setTimeout(90_000);
  const home = [
    building(20, 1, 8190, 8187, 2),
    building(21, 1, 8186, 8186, 2, 900, true, 1),
  ];
  await inputFixture(page, "webgl2", home);
  await selectWorker(page);
  const house = screen(8191, 8188);
  await page.keyboard.down("Shift");
  await page.mouse.click(house.x, house.y);
  await page.keyboard.up("Shift");
  const card = page.locator("#selection");
  await expect(card).toHaveAttribute("data-type", "mixed");
  await expect(page.locator("#selection-name")).toHaveText(
    "2 units and buildings selected",
  );
  await expect(page.locator("#stop-units")).toBeVisible();
  await expect(page.locator("#selected-icons .unit-token")).toHaveCount(2);
  await page
    .locator('#selected-icons [data-unit-id="20"]')
    .click({ modifiers: ["Shift"] });
  await expect(card).toHaveAttribute("data-type", "unit");
  const actor = async (kind: string, id: number) => {
    const point = await page.locator("#scene").evaluate((element, id) => {
      const actors = JSON.parse(
        element.getAttribute("data-actors") ?? "[]",
      ) as { id: number; x: number; y: number }[];
      return actors.find((actor) => actor.id === id);
    }, id);
    if (!point) throw new Error(kind + " missing");
    await page.mouse.click(point.x, point.y);
    await expect(card).toHaveAttribute("data-type", kind);
    await expect(page.locator("#command-grid button")).toHaveCount(0);
  };
  await actor("animal", 7);
  await expect(page.locator("#selection-load")).toHaveText(
    "140 food remaining",
  );
  await actor("enemy", 4);
  await expect(page.locator("#selection-owner")).toHaveText("Player 2");
  await actor("enemy", 21);
  await expect(page.locator("#selection-owner")).toBeVisible();
  await expect(page.locator("#selection-owner")).toHaveText("Player 2");
  const berry = screen(8191.5, 8194.5);
  await page.mouse.click(berry.x, berry.y);
  await expect(card).toHaveAttribute("data-type", "resource");
  await expect(page.locator("#selection-name")).toHaveText("Berry bush");
  await expect(page.locator("#selection-load")).toContainText("food remaining");
  await expect(page.locator("#unit-combat")).toBeHidden();
  await expect(page.locator("#command-grid button")).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(card).toHaveAttribute("data-type", "empty");
});
