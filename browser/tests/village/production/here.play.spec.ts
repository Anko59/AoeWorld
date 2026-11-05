import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { playerState, productionFixture } from "./fixture.js";
import { gameAssets } from "../../game-assets.js";
import { forceWebGl } from "../../rendering/backends.js";
import { openTab } from "../../rendering/fixtures/cockpit-tabs.js";

test("Units here charges real resources, trains a soldier and persists its rally on the real server", async ({
  page,
  request,
}) => {
  test.setTimeout(90_000);
  expect(
    (await request.post("/maps/reset?scenario=feudal-military")).status(),
  ).toBe(204);
  await forceWebGl(page);
  await gameAssets(page);
  await page.goto("/?play=1#x=128&y=128&zoom=.04");
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    "webgl2",
    { timeout: 30_000 },
  );
  await expect(page.locator("#connection")).toHaveText("connected");
  await openTab(page, "train");
  await page.locator('[aria-label="Unit type"] [data-id="2"]').click();
  await openTab(page, "train");
  await page.getByLabel("Unit count").fill("3");
  await page.getByRole("button", { name: "Units here", exact: true }).click();
  await page.mouse.click(635, 374);
  await expect(page.locator("#units-here-status")).toContainText("Accepted 3");
  // The current authoritative Militia table charges 50 food and 20 gold.
  await expect(page.locator("#stockpile-food")).toHaveText("9850");
  await expect(page.locator("#stockpile-gold")).toHaveText("9940");
  await expect(page.locator(".cockpit-army")).toContainText(/Militia: [1-9]/, {
    timeout: 40_000,
  });
  await page.reload();
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
  await page.evaluate(() => {
    location.hash = "x=122&y=131&zoom=.6";
  });
  await expect(page.locator("#scene")).toHaveAttribute("data-lod", "sprites");
  await expect
    .poll(async () => {
      const actors = JSON.parse(
        (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
      ) as { kind: string; x: number; y: number }[];
      const barracks = actors.find((a) => a.kind === "Barracks");
      if (!barracks) return false;
      await page.mouse.click(barracks.x, barracks.y);
      return page.locator("#selection").getAttribute("data-kind");
    })
    .toBe("Barracks");
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-rally-point",
    /.+/,
  );
  const encoded = await page.locator("#scene").getAttribute("data-rally-point");
  if (!encoded) throw new Error("Authoritative rally missing");
  const [rx, ry] = encoded
    .split(",")
    .map(Number)
    .map((n) => n / 1024);
  if (rx === undefined || ry === undefined)
    throw new Error("Rally position incomplete");
  expect(Math.abs(rx - 132.5)).toBeLessThan(1);
  expect(Math.abs(ry - 134.5)).toBeLessThan(1);
});

test("Units here shares ordinary training and rally orders across equally near eligible queues", async ({
  page,
}) => {
  test.setTimeout(90_000);
  const { commands, update } = await productionFixture(page, [
    { id: 1, kind: 6, x: 8191, y: 8186, queue: [2, 2] },
    { id: 2, kind: 6, x: 8191, y: 8186 },
    { id: 3, kind: 6, x: 8191, y: 8186, queue: [2] },
    { id: 4, kind: 6, x: 8184, y: 8192, player: 1 },
    { id: 5, kind: 6, x: 8184, y: 8198, complete: false },
  ]);
  update(playerState(0, 0, 4, 100));
  await expect(page.locator("#population")).toHaveText("4/100");
  await page.evaluate(() => {
    location.hash = "x=8192.5&y=8192.5&zoom=.04";
  });
  await expect(page.locator("#scene")).toHaveAttribute("data-lod", "markers");
  await openTab(page, "train");
  await page.locator('[aria-label="Unit type"] [data-id="2"]').click();
  await openTab(page, "train");
  await page.getByLabel("Unit count").fill("5");
  await page.getByRole("button", { name: "Units here", exact: true }).click();
  const before = commands.length;
  await page.mouse.click(635, 345);
  await expect.poll(() => commands.filter((c) => c[0] === 5).length).toBe(5);
  const orders = commands.slice(before);
  expect(orders.filter((c) => c[0] === 5).map((c) => c[2])).toEqual([
    2, 2, 2, 3, 1,
  ]);
  const rallies = orders.filter((c) => c[0] === 20);
  expect(rallies.map((c) => c[2]).sort()).toEqual([1, 2, 3]);
  expect(rallies.every((c) => c[3] === 0)).toBe(true);
  expect(new Set(rallies.map((c) => JSON.stringify(c.slice(4)))).size).toBe(1);
  await expect(page.locator("#units-here-status")).toContainText("Accepted 5");
  await expect(page.locator("#scene")).toHaveAttribute("data-lod", "markers");
  await page.getByRole("button", { name: "Units here", exact: true }).click();
  await page.keyboard.press("Escape");
  await expect(
    page.getByRole("button", { name: "Units here", exact: true }),
  ).toHaveAttribute("aria-pressed", "false");
});

test("Units here bounds human batches and displays accepted rather than planned training", async ({
  page,
}) => {
  const { commands, update } = await productionFixture(
    page,
    [{ id: 1, kind: 6, x: 8191, y: 8186 }],
    0,
    0,
    { trainResults: [0, 4, 0] },
  );
  update(playerState(0, 0, 4, 100));
  await page.evaluate(() => {
    location.hash = "x=8192.5&y=8192.5&zoom=.04";
  });
  await expect(page.locator("#scene")).toHaveAttribute("data-lod", "markers");
  await openTab(page, "train");
  await page.getByLabel("Unit count").fill("50");
  await page.getByRole("button", { name: "Units here", exact: true }).click();
  await expect(page.locator("#units-here-status")).toContainText("1 to 10");
  expect(commands.filter((c) => c[0] === 5 || c[0] === 20)).toHaveLength(0);
  await openTab(page, "train");
  await page.getByLabel("Unit count").fill("3");
  await page.getByRole("button", { name: "Units here", exact: true }).click();
  await page.mouse.click(635, 345);
  await expect(page.locator("#units-here-status")).toHaveText(
    "Accepted 2 of 3 Militia.",
  );
  expect(commands.filter((c) => c[0] === 5)).toHaveLength(3);
  expect(commands.filter((c) => c[0] === 20)).toHaveLength(1);
});

test("Units here sends no training when the server rejects its rally", async ({
  page,
}) => {
  const { commands, update } = await productionFixture(
    page,
    [{ id: 1, kind: 6, x: 8191, y: 8186 }],
    0,
    0,
    { rallyResult: 4 },
  );
  update(playerState(0, 0, 4, 100));
  await page.evaluate(() => {
    location.hash = "x=8192.5&y=8192.5&zoom=.04";
  });
  await expect(page.locator("#scene")).toHaveAttribute("data-lod", "markers");
  await openTab(page, "train");
  await page.getByLabel("Unit count").fill("3");
  await page.getByRole("button", { name: "Units here", exact: true }).click();
  await page.mouse.click(635, 345);
  await expect(page.locator("#units-here-status")).toHaveText(
    "Accepted 0 of 3 Militia.",
  );
  expect(commands.filter((c) => c[0] === 5)).toHaveLength(0);
});

test("Units here prioritizes the closest occupied queue over a farther idle producer", async ({
  page,
}) => {
  const { commands, update } = await productionFixture(page, [
    { id: 1, kind: 6, x: 8191, y: 8186, queue: [2] },
    { id: 2, kind: 6, x: 8300, y: 8300 },
  ]);
  update(playerState(0, 0, 4, 100));
  await page.evaluate(() => {
    location.hash = "x=8192.5&y=8192.5&zoom=.04";
  });
  await expect(page.locator("#scene")).toHaveAttribute("data-lod", "markers");
  await openTab(page, "train");
  await page.locator('[aria-label="Unit type"] [data-id="2"]').click();
  await openTab(page, "train");
  await page.getByLabel("Unit count").fill("1");
  await page.getByRole("button", { name: "Units here", exact: true }).click();
  await page.mouse.click(635, 345);
  await expect(page.locator("#units-here-status")).toContainText("Accepted 1");
  expect(commands.filter((c) => c[0] === 5).map((c) => c[2])).toEqual([1]);
  expect(commands.filter((c) => c[0] === 20).map((c) => c[2])).toEqual([1]);
});
