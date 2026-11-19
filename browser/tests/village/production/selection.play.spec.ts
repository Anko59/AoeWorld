import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { productionFixture } from "./fixture.js";
import { zoom } from "../../rendering/fixtures/bigview.js";

test.setTimeout(90_000);

test("Town Center queue rows leave room for the garrison panel", async ({
  page,
}) => {
  await productionFixture(page, [
    { id: 40, kind: 0, x: 8185, y: 8190, queue: [0, 0] },
    { id: 41, kind: 0, x: 8195, y: 8188, queue: [0] },
  ]);
  const actor = await page.locator("#scene").evaluate((canvas) =>
    (
      JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as {
        id: number;
        x: number;
        y: number;
      }[]
    ).find((actor) => actor.id === 40),
  );
  if (!actor) throw new Error("Town Center missing");
  await page.mouse.dblclick(actor.x, actor.y);
  const queues = page.locator("#training-multi");
  const garrison = page.locator("#garrison-panel");
  await expect(queues.locator(".training-row")).toHaveCount(2);
  await expect(garrison).toBeVisible();
  const queueBox = await queues.boundingBox();
  const garrisonBox = await garrison.boundingBox();
  if (!queueBox || !garrisonBox) throw new Error("Selection panels missing");
  expect(queueBox.x + queueBox.width).toBeLessThanOrEqual(garrisonBox.x);
});

test("double-click selects on-screen owned production buildings and balances queues", async ({
  page,
}) => {
  const buildings = [
    { id: 40, kind: 6, x: 8185, y: 8190, queue: [2, 2] },
    { id: 41, kind: 6, x: 8195, y: 8188, queue: [] as number[] },
    { id: 42, kind: 6, x: 8190, y: 8197, queue: [2] },
    { id: 43, kind: 6, x: 8190, y: 8183, player: 1 },
    { id: 44, kind: 6, x: 8250, y: 8250 },
    { id: 45, kind: 10, x: 8199, y: 8194 },
  ];
  const { commands } = await productionFixture(page, buildings);
  const actor = await page.locator("#scene").evaluate((canvas) =>
    (
      JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as {
        id: number;
        x: number;
        y: number;
      }[]
    ).find((a) => a.id === 40),
  );
  if (!actor) throw new Error("Barracks missing");
  await page.mouse.dblclick(actor.x, actor.y);
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "3");
  await expect(page.locator("#selection-name")).toHaveText(
    "3 Barracks selected",
  );
  const selected = await page.locator("#scene").evaluate((canvas) =>
    (
      JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as {
        id: number;
        selected: boolean;
      }[]
    )
      .filter((a) => a.selected)
      .map((a) => a.id)
      .sort(),
  );
  expect(selected).toEqual([40, 41, 42]);
  const rows = page.locator("#training-multi .training-row");
  await expect(rows).toHaveCount(3);
  await expect(rows.nth(0).locator(".unit-token")).toHaveCount(2);
  await expect(rows.nth(1)).toContainText("Empty");
  await expect(rows.nth(2).locator(".unit-token")).toHaveCount(1);
  await expect(page.locator("#training-queue")).toBeHidden();
  for (const width of [1280, 900]) {
    await page.setViewportSize({ width, height: 720 });
    expect(
      await rows.evaluateAll((entries) => {
        const portraits = document.querySelector("#selected-icons");
        const selection = document.querySelector("#selection-content");
        if (!portraits || !selection) {
          throw new Error("Selection panel or portraits missing");
        }
        const icons = portraits.getBoundingClientRect();
        const panel = selection.getBoundingClientRect();
        let bottom = panel.top;
        return entries.every((entry) => {
          const row = entry.getBoundingClientRect();
          const tokens = entry.querySelector(".training-tokens");
          const clear =
            row.top >= bottom - 1 &&
            row.top >= icons.bottom - 1 &&
            row.bottom <= panel.bottom + 1 &&
            row.height >= 16 &&
            tokens &&
            tokens.scrollWidth <= tokens.clientWidth;
          bottom = row.bottom;
          return clear;
        });
      }),
      `queue rows and selection portraits fit at ${width}px`,
    ).toBe(true);
  }
  await page.setViewportSize({ width: 1280, height: 720 });
  await expect(page.locator("#train-villager")).toBeEnabled();
  await expect(page.locator("#train-spearman")).toBeVisible();
  for (let count = 1; count <= 5; count++) {
    await page.locator("#train-villager").click();
    await expect
      .poll(() => commands.filter((c) => c[0] === 5).length)
      .toBe(count);
  }
  expect(commands.filter((c) => c[0] === 5).map((c) => c[2])).toEqual([
    41, 41, 42, 40, 41,
  ]);
  await expect(rows.nth(0).locator(".unit-token")).toHaveCount(3);
  await expect(rows.nth(1).locator(".unit-token")).toHaveCount(3);
  await expect(rows.nth(2).locator(".unit-token")).toHaveCount(2);
  // Strategic picking must leave the production group intact, then hand
  // ordinary double-click building picking back to the sprite view.
  await page.mouse.move(640, 300);
  await zoom(page, 3000, "map");
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "3");
  await zoom(page, -3000, "sprites");
  const restored = await page.locator("#scene").evaluate((canvas) =>
    (
      JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as {
        id: number;
        x: number;
        y: number;
      }[]
    ).find((a) => a.id === 40),
  );
  if (!restored) throw new Error("Barracks missing after strategic view");
  await page.mouse.dblclick(restored.x, restored.y);
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "3");
});
