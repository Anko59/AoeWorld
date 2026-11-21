import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { building, inputFixture } from "../input/fixture.js";
import { monitorCommands } from "../fixtures/menu-shortcuts.js";
import { freshHud } from "../fixtures/resources.js";

const kinds = [
  "Town Center",
  "House",
  "Lumber Camp",
  "Mining Camp",
  "Mill",
  "Farm",
  "Barracks",
  "Palisade Wall",
  "Outpost",
  "Dock",
  "Archery Range",
  "Stable",
  "Blacksmith",
  "Market",
  "Watch Tower",
  "Palisade Gate",
  "Castle",
  "Monastery",
  "University",
  "Siege Workshop",
  "Bridge",
  "Stone Wall",
  "Stone Gate",
  "Road",
  "Wonder",
  "Fish Trap",
  "Guard Tower",
  "Keep",
];
const points = kinds.map((_, index) => ({
  id: index + 40,
  x: 8193 + 5 * (index % 6),
  y: 8163 + 5 * Math.floor(index / 6),
  food: index === 5 || index === 25 ? 100 : undefined,
}));
const sides = [
  4, 2, 2, 2, 2, 3, 3, 1, 1, 3, 3, 3, 3, 4, 1, 4, 4, 3, 4, 4, 3, 1, 4, 1, 5, 2,
  1, 1,
];

test("every building has a card and its command keys activate their matching controls", async ({
  page,
}) => {
  test.setTimeout(180_000);
  await inputFixture(
    page,
    "webgl2",
    points.map(({ id, x, y, food }, kind) =>
      building(id, kind, x, y, sides[kind] ?? 1, 900, true, 0, food),
    ),
  );
  await expect(page.locator("#stockpile-wood")).toHaveText("2000");
  const checkShortcuts = await monitorCommands(page);
  for (const [kind, point] of points.entries()) {
    await page.evaluate(({ x, y }) => {
      location.hash = `x=${x + 1}&y=${y + 1}&zoom=.8`;
    }, point);
    await expect(page.locator("#minimap")).toHaveAttribute(
      "data-camera-x",
      String(point.x + 1),
    );
    await freshHud(page);
    const actor = await page.locator("#scene").evaluate((element, id) => {
      const actors = JSON.parse(
        element.getAttribute("data-actors") ?? "[]",
      ) as { id: number; x: number; y: number }[];
      return actors.find((actor) => actor.id === id);
    }, point.id);
    if (!actor) throw new Error(`Building missing: ${kinds[kind]}`);
    await page.mouse.click(actor.x, actor.y);
    await expect(page.locator("#selection-name")).toHaveText(kinds[kind] ?? "");
    await expect(page.locator("#building-status")).toHaveText(
      point.food === undefined
        ? "Complete"
        : `Complete · ${point.food} food remaining`,
    );
    if (kind === 0)
      await expect(page.locator("#selection-load")).toContainText(
        "Resource drop-off",
      );
    if (kind === 2)
      await expect(page.locator("#selection-load")).toHaveText("Wood drop-off");
    if (kind === 14 || kind === 16)
      await expect(page.locator("#selection-load")).toContainText(
        `Base pierce attack ${kind === 14 ? 5 : 11} · range 1–8`,
      );
    if (kind === 26 || kind === 27) {
      await expect(
        page.locator("#selection-portrait .art-icon"),
      ).toHaveAttribute("data-ui-id", String(kind === 26 ? 234 : 235));
      await expect(page.locator("#selection-load")).toContainText(
        /melee\/pierce armour/,
      );
    }
    await checkShortcuts(kinds[kind] ?? "");
  }
});
