import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { playerState, productionFixture } from "./fixture.js";

test.setTimeout(90_000);

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
