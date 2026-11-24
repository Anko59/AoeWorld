import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { inputFixture } from "../input/fixture.js";
import {
  productionBuilding,
  productionFixture,
} from "../production/fixture.js";

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

test("kingdom activity shows every type without scrollbars", async ({
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
  await expect(panel.getByRole("button")).toHaveCount(8);
  for (const lod of ["sprites", "markers"]) {
    const size = await panel.evaluate((node, level) => {
      document.documentElement.dataset.lod = level;
      const rows = node.querySelector(".activity-rows");
      if (!rows) throw new Error("Activity rows are missing");
      return {
        scrolls: [node, rows].some(
          (element) =>
            element.scrollHeight > element.clientHeight ||
            element.scrollWidth > element.clientWidth,
        ),
        visible: [...rows.children].every((row) => {
          const rect = row.getBoundingClientRect();
          return (
            rect.top >= 0 &&
            rect.left >= 0 &&
            rect.bottom <= innerHeight &&
            rect.right <= innerWidth
          );
        }),
      };
    }, lod);
    expect(size).toEqual({ scrolls: false, visible: true });
  }
});

test("kingdom activity stays above the far-view menus and uses the original panel frame", async ({
  page,
  request,
}) => {
  const originalArt = (
    await request.get("/asset-pack/buildings/interface.json")
  ).ok();
  await inputFixture(
    page,
    "webgpu",
    [productionBuilding({ id: 40, kind: 0, x: 8187, y: 8190, queue: [0] })],
    false,
    0,
    undefined,
    { originalArt },
  );
  await page.evaluate(() => {
    location.hash = "x=8192.5&y=8192.5&zoom=.04";
  });
  const panel = page.locator("#kingdom-activity");
  await expect(panel.getByRole("button", { name: /Villager/ })).toBeVisible();
  await expect(page.locator("#strategic-cockpit")).toBeVisible();
  const dock = page.locator("#far-dock");
  await expect(dock).toBeVisible();
  const layout = await panel.evaluate((node) => {
    const menus = ["strategic-cockpit", "far-dock"].map((id) => {
      const menu = document.getElementById(id);
      if (!menu) throw new Error("Far-view menu missing");
      return Number(getComputedStyle(menu).zIndex);
    });
    return { zIndex: Number(getComputedStyle(node).zIndex), menus };
  });
  expect(layout.zIndex).toBeGreaterThan(Math.max(...layout.menus));
  if (originalArt) {
    await expect(panel).toHaveCSS("border-image-source", /url\("blob:/);
    const [activityBox, dockBox] = await Promise.all([
      panel.boundingBox(),
      dock.boundingBox(),
    ]);
    if (!activityBox || !dockBox) throw new Error("HUD bounds missing");
    expect(activityBox.y).toBeGreaterThanOrEqual(dockBox.y + dockBox.height);
  }
});
