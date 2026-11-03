import { expect, type Page } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { centerVillage, freshHud } from "../fixtures/resources.js";

test.use({ video: { mode: "on", size: { width: 1280, height: 720 } } });

type Actor = { id: number; kind: string; x: number; y: number; task: string };
async function actors(page: Page): Promise<Actor[]> {
  return JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  );
}

async function selectWorkers(page: Page, ids: number[]) {
  await page.keyboard.press("Escape");
  const villagers = (await actors(page)).filter((a) => a.kind === "Villager");
  await page.mouse.move(
    Math.min(...villagers.map((a) => a.x)) - 20,
    Math.min(...villagers.map((a) => a.y)) - 20,
  );
  await page.mouse.down();
  await page.mouse.move(
    Math.max(...villagers.map((a) => a.x)) + 20,
    Math.max(...villagers.map((a) => a.y)) + 20,
    { steps: 3 },
  );
  await page.mouse.up();
  const tokens = page.locator("#selected-icons .unit-token");
  await expect.poll(() => tokens.count()).toBeGreaterThanOrEqual(3);
  if (ids.length === 1) {
    await page.locator(`[data-unit-id="${ids[0]}"]`).click();
  } else {
    const selected = await tokens.evaluateAll((nodes) =>
      nodes.map((node) => Number(node.getAttribute("data-unit-id"))),
    );
    for (const id of selected.filter((id) => !ids.includes(id))) {
      await page
        .locator(`[data-unit-id="${id}"]`)
        .click({ modifiers: ["Shift"] });
    }
  }
  await expect(page.locator("#selection")).toHaveAttribute(
    "data-count",
    String(ids.length),
  );
}

async function place(page: Page, kind: string, x: number, y: number) {
  await page.locator("#build-menu").click();
  await page.locator(`#build-${kind}`).click();
  await page.mouse.move(x, y);
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-placement-valid",
    "true",
  );
  await page.keyboard.down("Shift");
  await page.mouse.click(x, y);
  await page.keyboard.up("Shift");
  await page.keyboard.press("Escape");
}

test("farm staffing keeps a single builder's queued house", async ({
  page,
  request,
}, info) => {
  test.setTimeout(180_000);
  await page.setViewportSize({ width: 1280, height: 720 });
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected");
  await centerVillage(page);
  const workers = (await actors(page))
    .filter((a) => a.kind === "Villager")
    .sort((a, b) => a.id - b.id)
    .slice(0, 2)
    .map((a) => a.id);
  expect(workers).toHaveLength(2);
  const first = workers[0];
  if (first === undefined) throw new Error("First worker missing");
  await selectWorkers(page, workers);
  await page.keyboard.press("Control+1");
  await place(page, "mill", 900, 380);
  await expect
    .poll(async () => (await actors(page)).some((a) => a.kind === "Mill"))
    .toBe(true);
  const mill = (await actors(page)).find((a) => a.kind === "Mill");
  if (!mill) throw new Error("Mill missing");
  await page.mouse.click(mill.x, mill.y);
  await expect(page.locator("#building-status")).toContainText("Complete", {
    timeout: 70_000,
  });
  await page.keyboard.press("1");
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "2");
  await place(page, "farm", 250, 200);
  // Escape ends placement and clears selection; recall the moving builders.
  await page.keyboard.press("1");
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "2");
  await page.locator(`[data-unit-id="${first}"]`).click({ force: true });
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "1");
  expect((await actors(page)).find((a) => a.id === first)?.task).toMatch(
    /Moving|Building/,
  );
  await place(page, "house", 1050, 200);
  await expect
    .poll(async () => (await actors(page)).some((a) => a.kind === "House"))
    .toBe(true);
  const house = (await actors(page)).find((a) => a.kind === "House");
  if (!house) throw new Error("Queued house missing");
  await page.mouse.click(house.x, house.y);
  await expect(page.locator("#building-status")).toContainText("Complete", {
    timeout: 70_000,
  });
  await expect(page.locator("#workers-food")).toHaveText("1");
  await page.screenshot({
    path: `../.cache/showcase/farm-unique-queue-${info.project.name}.png`,
  });
  await page.waitForTimeout(2000);
  const video = page.video();
  await page.close();
  await video?.saveAs(
    `../.cache/showcase/farm-unique-queue-${info.project.name}.webm`,
  );
});

test("queued house orders include a selected farmer and idle villager", async ({
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
  const workers = (await actors(page))
    .filter((a) => a.kind === "Villager")
    .sort((a, b) => a.id - b.id)
    .slice(0, 2)
    .map((a) => a.id);
  const first = workers[0];
  if (first === undefined) throw new Error("First worker missing");
  await selectWorkers(page, [first]);
  await place(page, "farm", 250, 200);
  await expect(page.locator("#workers-food")).toHaveText("1", {
    timeout: 30_000,
  });
  await selectWorkers(page, workers);
  await place(page, "house", 1050, 200);
  await expect(page.locator("#workers-food")).toHaveText("0", {
    timeout: 5000,
  });
  const house = (await actors(page)).find((a) => a.kind === "House");
  if (!house) throw new Error("Queued house missing");
  await page.mouse.click(house.x, house.y);
  await expect(page.locator("#building-status")).toContainText("Complete", {
    timeout: 70_000,
  });
  await page.screenshot({
    path: `../.cache/showcase/farmer-group-order-${info.project.name}.png`,
  });
  await page.waitForTimeout(2000);
  const video = page.video();
  await page.close();
  await video?.saveAs(
    `../.cache/showcase/farmer-group-order-${info.project.name}.webm`,
  );
});

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
