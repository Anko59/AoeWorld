import {
  expect,
  type Page,
  type APIRequestContext,
  type TestInfo,
} from "@playwright/test";
import { gameAssets } from "../../game-assets.js";
import { centerVillage, freshHud } from "../fixtures/resources.js";
import { forceCanvas } from "../../rendering/backends.js";

type Actor = {
  id: number;
  owned: boolean;
  kind: string;
  x: number;
  y: number;
  hp: number;
  maxHp: number;
  task: string;
};
async function actors(page: Page): Promise<Actor[]> {
  return JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as Actor[];
}
// Soldiers now engage on their own and move between reading a position and
// the click, so re-read and click until the selection takes.
async function select(page: Page, kind: string) {
  await expect(async () => {
    const actor = (await actors(page)).find(
      (a) => a.kind === kind && a.x > 0 && a.x < 1280 && a.y > 54 && a.y < 512,
    );
    if (!actor) throw new Error(`Visible ${kind} missing`);
    await page.mouse.click(actor.x, actor.y);
    await expect(page.locator("#selection")).toHaveAttribute("data-kind", kind, {
      timeout: 1_000,
    });
  }).toPass({ timeout: 15_000 });
}

export async function militaryAttack(
  page: Page,
  request: APIRequestContext,
  info: TestInfo,
) {
  info.setTimeout(90_000);
  if (info.project.name === "canvas") await forceCanvas(page);
  expect(
    (await request.post("/maps/reset?scenario=feudal-military")).status(),
  ).toBe(204);
  await gameAssets(page);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
  await centerVillage(page);
  await page.mouse.move(640, 360);
  await page.mouse.wheel(0, 288);
  await freshHud(page);
  await select(page, "ArcheryRange");
  await page.locator("#train-archer").click();
  await select(page, "Barracks");
  await page.locator("#train-spearman").click();
  await select(page, "Blacksmith");
  await page.locator("#research-fletching").click();
  await expect
    .poll(async () => (await actors(page)).some((a) => a.kind === "Archer"), {
      timeout: 20_000,
    })
    .toBe(true);
  await expect
    .poll(async () => (await actors(page)).some((a) => a.kind === "Spearman"))
    .toBe(true);
  // The scenario's enemy Militia and Scout now raid the village; the new
  // Spearman fights back without an order.
  const enemies = (await actors(page)).filter((a) => !a.owned);
  const militia = enemies.find((a) => a.kind === "Militia");
  const scout = enemies.find((a) => a.kind === "Scout");
  if (!militia || !scout) throw new Error("Enemy Militia or Scout missing");
  await expect
    .poll(async () => damaged(page, militia.id), {
      timeout: 40_000,
      message: "the idle Spearman fights back against the raiding Militia",
    })
    .toBe(true);
  await expect(page.locator("#research-fletching")).toBeDisabled();
  await select(page, "Archer");
  await expect(page.locator("#unit-combat")).toContainText("Attack 5");
  await expect(page.locator("#unit-combat")).toContainText("Range 5");
  // An explicit order on the untouched enemy Scout.
  const target = (await actors(page)).find((a) => a.id === scout.id);
  if (!target) throw new Error("Enemy Scout gone before the Archer's order");
  await page.mouse.click(target.x, target.y, { button: "right" });
  await expect
    .poll(async () => damaged(page, scout.id), {
      timeout: 20_000,
      message: "the ordered Archer damages the enemy Scout",
    })
    .toBe(true);
}

// Damaged, or killed and removed.
async function damaged(page: Page, id: number) {
  return !(await actors(page)).some((a) => a.id === id && a.hp === a.maxHp);
}
