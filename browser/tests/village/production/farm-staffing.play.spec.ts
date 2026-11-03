import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { centerVillage, freshHud } from "../fixtures/resources.js";

test.use({ video: { mode: "on", size: { width: 1280, height: 720 } } });

test("three queued farms retain three farmers without another order", async ({
  page,
  request,
}, info) => {
  test.setTimeout(120_000);
  await page.setViewportSize({ width: 1280, height: 720 });
  expect(
    (await request.post("/maps/reset?scenario=feudal-economy")).status(),
  ).toBe(204);
  await gameAssets(page);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected");
  await centerVillage(page);
  const actors = async () =>
    JSON.parse(
      (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
    ) as { id: number; kind: string; x: number; y: number; task: string }[];
  const villager = (await actors()).find((a) => a.kind === "Villager");
  if (!villager) throw new Error("Starting villager missing");
  await page.mouse.dblclick(villager.x, villager.y);
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "3");
  await page.locator("#build-menu").click();
  await page.locator("#build-farm").click();
  // Find three disjoint valid sites before issuing any order. This keeps
  // accelerated scenario construction from finishing during site discovery.
  const sites: { x: number; y: number }[] = [];
  for (const [x, y] of [
    [250, 200],
    [1050, 200],
    [250, 420],
    [1050, 420],
    [650, 100],
  ] as const) {
    await page.mouse.move(x, y);
    await freshHud(page);
    await freshHud(page);
    if (
      (await page.locator("#scene").getAttribute("data-placement-valid")) ===
      "true"
    )
      sites.push({ x, y });
    if (sites.length === 3) break;
  }
  expect(sites).toHaveLength(3);
  await page.keyboard.down("Shift");
  for (const { x, y } of sites) await page.mouse.click(x, y);
  await page.keyboard.up("Shift");
  await expect
    .poll(async () => (await actors()).filter((a) => a.kind === "Farm").length)
    .toBe(3);
  await expect(page.locator("#workers-food")).toHaveText("3", {
    timeout: 60_000,
  });
  await expect
    .poll(async () => (await actors()).filter((a) => a.kind === "Farm").length)
    .toBe(3);
  await expect
    .poll(
      async () =>
        (await actors()).filter(
          (a) => a.kind === "Villager" && a.task === "Farming",
        ).length,
      { timeout: 30_000 },
    )
    .toBe(3);
  await page.screenshot({
    path: `../.cache/showcase/farm-staffing-${info.project.name}.png`,
  });
  await page.waitForTimeout(3000);
  const video = page.video();
  await page.close();
  await video?.saveAs(
    `../.cache/showcase/farm-staffing-${info.project.name}.webm`,
  );
});
