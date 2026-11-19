import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { playerState, productionFixture } from "./fixture.js";

test.setTimeout(90_000);
test.use({ video: "on" });

test("Shift-click and Shift+Q each queue five units up to available slots", async ({
  page,
}) => {
  const center = { id: 40, kind: 0, x: 8187, y: 8190, queue: [0, 0] };
  const { commands } = await productionFixture(page, [center]);
  const actor = await page.locator("#scene").evaluate((canvas) =>
    (
      JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as {
        id: number;
        x: number;
        y: number;
      }[]
    ).find((a) => a.id === 40),
  );
  if (!actor) throw new Error("Town Center missing");
  await page.mouse.click(actor.x, actor.y);
  const train = page.locator("#train-villager");
  const tokens = page.locator("#training-queue .unit-token");
  await expect(train).toBeEnabled();
  await train.click({ modifiers: ["Shift"] });
  await expect(tokens).toHaveCount(7);
  await page.keyboard.press("Shift+Q");
  await expect(tokens).toHaveCount(12);
  await train.click({ modifiers: ["Shift"] });
  await expect(tokens).toHaveCount(15);
  await expect(train).toBeDisabled();
  expect(commands.filter((c) => c[0] === 5).map((c) => c.slice(2))).toEqual(
    Array.from({ length: 13 }, () => [40, 0]),
  );
});

test("Shift-click on a secondary training button fills only affordable slots", async ({
  page,
}) => {
  const barracks = { id: 40, kind: 6, x: 8187, y: 8190 };
  const feudalAge = 2 ** 2;
  const { commands, update } = await productionFixture(
    page,
    [barracks],
    1,
    feudalAge,
  );
  update(playerState(1, feudalAge, 5, 5, 70));
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
  await page.mouse.click(actor.x, actor.y);
  await expect(page.locator("#stockpile-food")).toHaveText("70");
  await page.locator("#train-spearman").click({ modifiers: ["Shift"] });
  await expect(page.locator("#training-queue .unit-token")).toHaveCount(2);
  expect(commands.filter((c) => c[0] === 5).map((c) => c.slice(2))).toEqual([
    [40, 6],
    [40, 6],
  ]);
});

test("a population-blocked building accepts fifteen units and explains housing", async ({
  page,
}) => {
  const center = { id: 40, kind: 0, x: 8187, y: 8190, queue: [0] };
  // Start with one prepaid unit at the population cap.
  const { commands, update } = await productionFixture(page, [center]);
  update(playerState(0, 0, 5, 5));
  const actor = await page.locator("#scene").evaluate((canvas) =>
    (
      JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as {
        kind: string;
        x: number;
        y: number;
      }[]
    ).find((a) => a.kind === "TownCenter"),
  );
  if (!actor) throw new Error("Town Center missing");
  await page.mouse.click(actor.x, actor.y);
  await expect(page.locator("#selection-name")).toHaveText("Town Center");
  const train = page.locator("#train-villager");
  await expect(train).toBeEnabled();
  await expect(page.locator("#training-label")).toContainText(
    "Waiting for housing",
  );
  for (let count = 2; count <= 15; count++) {
    await train.click();
    await expect(page.locator("#training-queue .unit-token")).toHaveCount(
      count,
    );
  }
  await expect(train).toBeDisabled();
  await expect(train).toHaveAttribute("title", /queue is full/);
  expect(commands.filter((c) => c[0] === 5)).toHaveLength(14);
  await page.setViewportSize({ width: 900, height: 720 });
  await expect(page.locator("#training-queue .unit-token")).toHaveCount(15);
  const fits = await page.locator("#training-queue").evaluate((queue) => {
    const bounds = queue.getBoundingClientRect();
    return Array.from(queue.children).every((token) => {
      const rect = token.getBoundingClientRect();
      return (
        rect.width >= 24 &&
        rect.left >= bounds.left - 1 &&
        rect.right <= bounds.right + 1 &&
        rect.bottom <= innerHeight
      );
    });
  });
  expect(
    fits,
    "all fifteen queue icons fit the panel at a narrow viewport",
  ).toBe(true);
});
