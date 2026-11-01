import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { fiveVillagerFixture } from "./fixtures/village.js";
import { workerFixture } from "./fixtures/worker.js";
import { freshHud } from "./fixtures/resources.js";

for (const [edge, y] of [
  ["top", 1],
  ["bottom", 719],
] as const) {
  test(`${edge} HUD outer edge still pans at 1280 by 720 and controls work`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1280, height: 720 });
    await gameAssets(page);
    await fiveVillagerFixture(page);
    await page.goto("/#x=8192&y=8192&zoom=.35");
    await expect(page.locator("#population")).toHaveText("6");
    const camera = () => page.locator("#minimap").getAttribute("data-camera-y");
    await page.mouse.move(640, 360);
    await freshHud(page);
    await freshHud(page);
    const before = Number(await camera());
    await page.mouse.move(640, y);
    const delta = () => camera().then((value) => Number(value) - before);
    if (edge === "top") await expect.poll(delta).toBeLessThan(-0.5);
    else await expect.poll(delta).toBeGreaterThan(0.5);
    await page.mouse.move(640, 360);
    await page.locator("#menu-toggle").click();
    await expect(page.getByRole("dialog", { name: "Game menu" })).toBeVisible();
    await page.locator("#resume-game").click();
    await expect(page.getByRole("dialog", { name: "Game menu" })).toBeHidden();
  });
}

test("a stopped villager retains its load without counting as a wood worker", async ({
  page,
}) => {
  await gameAssets(page);
  await workerFixture(page);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
  await expect(page.locator("#population")).toHaveText("3");
  await expect(page.locator("#workers-wood")).toHaveText("0");
  const worker = await page.locator("#scene").evaluate((canvas) =>
    (
      JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as {
        id: number;
        x: number;
        y: number;
      }[]
    ).find((a) => a.id === 3),
  );
  if (!worker) throw new Error("Stopped worker missing");
  await page.mouse.click(worker.x, worker.y);
  await expect(page.locator("#selection-load")).toContainText("3/10 wood");
});

test("reconnecting preserves an offscreen zero-load gold worker without terrain chunks", async ({
  page,
}) => {
  await gameAssets(page);
  await workerFixture(page);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
  await expect(page.locator("#population")).toHaveText("3");
  await expect(page.locator("#workers-gold")).toHaveText("1");
  await page.reload();
  await expect(page.locator("#workers-gold")).toHaveText("1");
  await expect(page.locator("#minimap")).toHaveAttribute("data-map-hash", "");
});

test("Escape disarms Move before the next selection click", async ({
  page,
}) => {
  await gameAssets(page);
  await fiveVillagerFixture(page);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
  await expect(page.locator("#population")).toHaveText("6");
  const worker = await page.locator("#scene").evaluate((canvas) =>
    (
      JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as {
        kind: string;
        x: number;
        y: number;
      }[]
    ).find((actor) => actor.kind === "Villager"),
  );
  if (!worker) throw new Error("Villager missing");
  await page.mouse.click(worker.x, worker.y);
  await expect(page.locator("#selection")).toHaveAttribute(
    "data-kind",
    "Villager",
  );
  await page.locator("#move-units").click();
  await page.locator("#scene").focus();
  await page.keyboard.press("Escape");
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "0");
  await page.mouse.click(worker.x, worker.y);
  await expect(page.locator("#selection")).toHaveAttribute(
    "data-kind",
    "Villager",
  );
});

test("setup-only maps owns quitting and exposes no in-game creator", async ({
  page,
}) => {
  await gameAssets(page);
  await fiveVillagerFixture(page);
  await page.goto("/?play=1#x=8192&y=8192&zoom=.35");
  await expect(page.locator("#population")).toHaveText("6");
  await expect(
    page.getByRole("button", { name: "Open map creator" }),
  ).toHaveCount(0);
  await page.locator("#menu-toggle").click();
  await page.getByRole("button", { name: "Quit to Main Menu" }).click();
  await expect(page.locator("#main-menu")).toBeVisible();
  await expect(page).not.toHaveURL(/play=1|#/);
});
