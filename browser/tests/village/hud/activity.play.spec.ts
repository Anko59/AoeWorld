import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { productionFixture } from "../production/fixture.js";

test("kingdom activity groups all owned queues, focuses a producer, and clears completed work", async ({
  page,
}) => {
  const buildings = [
    { id: 40, kind: 0, x: 8187, y: 8190, queue: [0, 0], progress: 125 },
    { id: 41, kind: 0, x: 8250, y: 8250, queue: [0], progress: 250 },
    { id: 42, kind: 0, x: 8180, y: 8190, player: 1, queue: [0] },
    { id: 43, kind: 0, x: 8195, y: 8195, research: 0, progress: 100 },
  ];
  const { update } = await productionFixture(page, buildings);
  const panel = page.locator("#kingdom-activity");
  await expect(panel).toBeVisible();
  const villagers = panel.getByRole("button", { name: /Villager/ });
  await expect(villagers).toContainText("3");
  await expect(villagers.locator("progress")).toHaveAttribute("value", "500");
  await expect(panel.getByRole("button", { name: /Loom/ })).toBeVisible();
  await villagers.click();
  await expect(page.locator("#selection")).toHaveAttribute(
    "data-kind",
    "TownCenter",
  );
  await expect
    .poll(async () => {
      const actors = JSON.parse(
        (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
      ) as { id: number; selected: boolean }[];
      return actors.find((actor) => actor.id === 41)?.selected;
    })
    .toBe(true);
  const remote = buildings[1];
  if (!remote) throw new Error("Missing offscreen producer fixture");
  remote.progress = 375;
  update();
  await expect(villagers.locator("progress")).toHaveAttribute("value", "750");
  for (const b of buildings) {
    b.queue = [];
    delete b.research;
  }
  update();
  await expect(panel).toBeHidden();
});

test("kingdom activity caps visible types and reports overflow", async ({
  page,
}) => {
  await productionFixture(
    page,
    [
      { id: 40, kind: 0, x: 8187, y: 8190, queue: [0] },
      { id: 41, kind: 6, x: 8200, y: 8190, queue: [2, 6, 7, 8] },
      { id: 42, kind: 10, x: 8210, y: 8190, queue: [4, 5] },
      { id: 43, kind: 11, x: 8220, y: 8190, queue: [1] },
    ],
    2,
    (1 << 2) | (1 << 14),
  );
  const panel = page.locator("#kingdom-activity");
  await expect(panel.getByRole("button")).toHaveCount(6);
  await expect(panel).toContainText("+2 more");
  const bounds = await panel.boundingBox();
  if (!bounds) throw new Error("Activity panel has no bounds");
  expect(bounds.height).toBeLessThanOrEqual(280);
});
