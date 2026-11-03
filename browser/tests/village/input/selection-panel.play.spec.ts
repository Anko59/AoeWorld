import { expect, type Page } from "@playwright/test";
import { test } from "../../../server.js";
import { building, inputFixture, screen } from "./fixture.js";

// The default fixture owns a Scout (id 0) and two Villagers (ids 2 and 3).
// A box around all three selects the mixed group shown in the bottom panel.
const tokens = (page: Page) => page.locator("#selected-icons .unit-token");
const token = (page: Page, id: number) =>
  page.locator(`#selected-icons .unit-token[data-unit-id="${id}"]`);
const tokenIds = (page: Page) =>
  tokens(page).evaluateAll((nodes) =>
    nodes
      .map((node) => Number(node.getAttribute("data-unit-id")))
      .sort((a, b) => a - b),
  );

async function boxAll(page: Page) {
  await page.mouse.move(600, 330);
  await page.mouse.down();
  await page.mouse.move(760, 500, { steps: 3 });
  await page.mouse.up();
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "3");
  await expect.poll(() => tokenIds(page)).toEqual([0, 2, 3]);
}

test("the multi-selection panel narrows a group with click, Shift, Ctrl and Ctrl+Shift", async ({
  page,
}, info) => {
  test.setTimeout(120_000);
  await inputFixture(
    page,
    info.project.name === "webgpu" ? "webgpu" : "webgl2",
  );

  // Shift+click removes exactly that unit.
  await boxAll(page);
  await token(page, 0).click({ modifiers: ["Shift"] });
  await expect.poll(() => tokenIds(page)).toEqual([2, 3]);
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "2");

  // Ctrl+click keeps only units of that type and drops the rest.
  await boxAll(page);
  await token(page, 2).click({ modifiers: ["Control"] });
  await expect.poll(() => tokenIds(page)).toEqual([2, 3]);
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "2");
  await expect(page.locator("#selection-name")).toHaveText(
    "2 Villagers selected",
  );

  await page.screenshot({
    path: `../.cache/showcase/selection-portraits-${info.project.name}.png`,
  });

  // Ctrl+Shift+click removes every unit of that type and keeps the rest.
  await boxAll(page);
  await token(page, 2).click({ modifiers: ["Control", "Shift"] });
  // One unit left: the panel shows its single-unit view, with no portraits.
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "1");
  await expect(page.locator("#selection")).toHaveAttribute(
    "data-kind",
    "Scout",
  );
  await expect.poll(() => tokenIds(page)).toEqual([]);

  // A plain click selects only that unit, leaving the panel in single mode.
  await boxAll(page);
  await token(page, 3).click();
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "1");
  await expect(page.locator("#selection-name")).toHaveText("Villager");
  await expect.poll(() => tokenIds(page)).toEqual([]);
});

test("building portraits keep building selection when narrowing a group", async ({
  page,
}, info) => {
  test.setTimeout(90_000);
  await inputFixture(
    page,
    info.project.name === "webgpu" ? "webgpu" : "webgl2",
    [building(20, 1, 8190, 8187, 2), building(21, 1, 8194, 8188, 2)],
  );
  const house = screen(8191, 8188);
  await page.mouse.dblclick(house.x, house.y);
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "2");
  await token(page, 20).click({ modifiers: ["Shift"] });
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "1");
  await expect(page.locator("#selection")).toHaveAttribute(
    "data-kind",
    "House",
  );
  await expect
    .poll(() =>
      page.locator("#scene").evaluate((el) =>
        (
          JSON.parse(el.getAttribute("data-actors") ?? "[]") as {
            id: number;
            selected: boolean;
          }[]
        )
          .filter((a) => a.selected)
          .map((a) => a.id),
      ),
    )
    .toEqual([21]);
  await page.screenshot({
    path: `../.cache/showcase/building-portrait-${info.project.name}.png`,
  });
});
