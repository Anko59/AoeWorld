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
async function select(page: Page, kind: string) {
  const actor = (await actors(page)).find(
    (a) => a.kind === kind && a.x > 0 && a.x < 1280 && a.y > 54 && a.y < 512,
  );
  if (!actor) throw new Error(`Visible ${kind} missing`);
  await page.mouse.click(actor.x, actor.y);
  await expect(page.locator("#selection")).toHaveAttribute("data-kind", kind);
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
  await expect(page.locator("#research-fletching")).toBeDisabled();
  await select(page, "Archer");
  await expect(page.locator("#unit-combat")).toContainText("Attack 5");
  await expect(page.locator("#unit-combat")).toContainText("Range 5");
  const enemy = (await actors(page)).find((a) => a.kind === "Militia");
  if (!enemy) throw new Error("Enemy target missing");
  await page.mouse.click(enemy.x, enemy.y, { button: "right" });
  await expect
    .poll(
      async () =>
        (await actors(page)).some((a) => a.id === enemy.id && a.hp < a.maxHp),
      { timeout: 20_000 },
    )
    .toBe(true);
  await select(page, "Spearman");
  const scout = (await actors(page)).filter((a) => a.kind === "Scout").at(-1);
  if (!scout) throw new Error("Enemy scout missing");
  await page.mouse.click(scout.x, scout.y, { button: "right" });
  await expect
    .poll(
      async () =>
        (await actors(page)).some((a) => a.id === scout.id && a.hp < a.maxHp),
      { timeout: 20_000 },
    )
    .toBe(true);
}
