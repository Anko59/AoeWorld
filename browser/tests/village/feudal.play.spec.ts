import { expect, type Page } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { centerVillage } from "./fixtures/resources.js";
import { workerFixture } from "./fixtures/worker.js";
import { forceCanvas } from "../rendering/backends.js";

type Actor = { kind: string; x: number; y: number };
async function actors(page: Page): Promise<Actor[]> {
  return JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as Actor[];
}
async function select(page: Page, kind: string) {
  const actor = (await actors(page)).find((a) => a.kind === kind);
  if (!actor) throw new Error(`${kind} missing`);
  await page.mouse.click(actor.x, actor.y);
}

// The explicit server scenario starts rich in Dark Age with a Mill, Lumber
// Camp and Barracks. Its one-Hz game clock runs under the normal 20-Hz ticker.
// Commands, construction, age-up, costs and research are all authoritative.
test("reach Feudal, build a Blacksmith and research Double-Bit Axe", async ({
  page,
  request,
}, info) => {
  test.setTimeout(90_000);
  if (info.project.name === "canvas") await forceCanvas(page);
  expect(
    (await request.post("/maps/reset?scenario=feudal-economy")).status(),
  ).toBe(204);
  await gameAssets(page);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
  if (info.project.name === "canvas")
    await expect(page.locator("#playground")).toHaveAttribute(
      "data-renderer",
      "canvas2d",
    );
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-fog-disabled",
    "false",
  );
  await centerVillage(page);
  await select(page, "Villager");
  await page.locator("#build-menu").click();
  await expect(page.locator("#build-blacksmith")).toBeDisabled();
  await expect(page.locator("#build-blacksmith")).toHaveAttribute(
    "title",
    /requires Feudal Age/,
  );
  await page.keyboard.press("v");
  await expect(page.locator("#build-archery-range")).toBeVisible();
  await expect(page.locator("#build-archery-range")).toBeDisabled();
  await expect(page.locator("#build-palisade-gate")).toBeEnabled();
  await expect(page.locator("#build-archery-range kbd")).toHaveText("W");
  await page.keyboard.press("Escape");
  await select(page, "LumberCamp");
  await expect(page.locator("#research-double-bit-axe")).toBeVisible();
  await expect(page.locator("#research-double-bit-axe")).toBeDisabled();
  await expect(page.locator("#research-double-bit-axe")).toHaveAttribute(
    "title",
    /requires Feudal Age/,
  );
  await centerVillage(page, true);
  await page.locator("#research-feudal-age").click();
  await expect(page.locator("#age")).toContainText("Feudal Age", {
    timeout: 20_000,
  });
  await select(page, "Villager");
  await page.locator("#build-menu").click();
  await expect(page.locator("#build-blacksmith")).toBeEnabled();
  await page.keyboard.press("s"); // DE economic grid: B, S = Blacksmith
  let placed = false;
  for (const [x, y] of [
    [920, 360],
    [900, 260],
    [400, 240],
    [420, 420],
    [850, 420],
    [640, 180],
  ] as const) {
    await page.mouse.move(x, y);
    await expect
      .poll(() => page.locator("#scene").getAttribute("data-placement-valid"))
      .toMatch(/true|false/);
    await page.waitForTimeout(150);
    if (
      (await page.locator("#scene").getAttribute("data-placement-valid")) ===
      "true"
    ) {
      await page.mouse.click(x, y);
      placed = true;
      break;
    }
  }
  expect(placed).toBe(true);
  await expect
    .poll(async () => (await actors(page)).some((a) => a.kind === "Blacksmith"))
    .toBe(true);
  await select(page, "Blacksmith");
  await expect(page.locator("#building-status")).toContainText("Complete", {
    timeout: 20_000,
  });
  await expect(page.locator("#building-hp")).toContainText("1800");
  await select(page, "LumberCamp");
  await expect(page.locator("#research-double-bit-axe")).toBeEnabled();
  const wood = Number(await page.locator("#stockpile-wood").textContent());
  await page.locator("#research-double-bit-axe").click();
  await expect(page.locator("#stockpile-wood")).toHaveText(String(wood - 50));
  await expect(page.locator("#research-status")).toContainText(
    "Double-Bit Axe",
  );
  await expect(page.locator("#research-status")).toHaveText("", {
    timeout: 10_000,
  });
  await expect(page.locator("#research-double-bit-axe")).toBeDisabled();
});

test("Feudal HUD preserves Militia identity and shows Wheelbarrow's thirteen-resource capacity", async ({
  page,
}, info) => {
  if (info.project.name === "canvas") await forceCanvas(page);
  await gameAssets(page);
  await workerFixture(page, true);
  await page.goto("/?play=1");
  await expect(page.locator("#age")).toContainText("Feudal Age");
  await expect
    .poll(async () => (await actors(page)).some((a) => a.kind === "Militia"))
    .toBe(true);
  await select(page, "Militia");
  await expect(page.locator("#selection-name")).toHaveText("Militia");
  const worker = (await actors(page)).find(
    (a) =>
      a.kind === "Villager" && a.x > 0 && a.x < 1280 && a.y > 54 && a.y < 512,
  );
  if (!worker) throw new Error("Visible loaded worker missing");
  await page.mouse.click(worker.x, worker.y);
  await expect(page.locator("#selection-load")).toContainText("13/13 wood");
});
