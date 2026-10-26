import { expect, type Page } from "@playwright/test";
import { isolatedTest as test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { centerVillage } from "../fixtures/resources.js";
import { forceCanvas, forceWebGl } from "../../rendering/backends.js";

async function select(page: Page, kind: string) {
  const actors = JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as { kind: string; x: number; y: number }[];
  const actor = actors.find((a) => a.kind === kind);
  if (!actor) throw new Error(`${kind} missing`);
  await page.mouse.click(actor.x, actor.y);
}

test("Castle Age unlocks native build commands, knights and tier two research", async ({
  page,
  request,
}, info) => {
  test.setTimeout(120_000);
  if (info.project.name === "canvas") await forceCanvas(page);
  if (info.project.name === "firefox-webgl2") await forceWebGl(page);
  expect(
    (await request.post("/maps/reset?scenario=feudal-military")).status(),
  ).toBe(204);
  await gameAssets(page);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
  await centerVillage(page, true);
  await expect(page.locator("#research-castle-age")).toBeEnabled();
  await select(page, "Stable");
  await expect(page.locator("#train-knight")).toBeDisabled();
  await expect(page.locator("#train-knight")).toHaveAttribute(
    "title",
    /requires Castle Age/,
  );
  await centerVillage(page, true);
  const food = Number(await page.locator("#stockpile-food").textContent());
  const gold = Number(await page.locator("#stockpile-gold").textContent());
  await page.locator("#research-castle-age").click();
  await expect(page.locator("#stockpile-food")).toHaveText(String(food - 800));
  await expect(page.locator("#stockpile-gold")).toHaveText(String(gold - 200));
  await expect(page.locator("#age")).toContainText("Castle Age", {
    timeout: 50_000,
  });
  await select(page, "Stable");
  await expect(page.locator("#train-knight")).toBeEnabled();
  await page.locator("#train-knight").click();
  await expect
    .poll(
      async () =>
        (
          (await page.locator("#scene").getAttribute("data-actors")) ?? ""
        ).includes('"kind":"Knight"'),
      { timeout: 15_000, intervals: [250, 500, 1000] },
    )
    .toBe(true);
  await select(page, "Knight");
  await expect(page.locator("#selection-name")).toHaveText("Knight");
  await select(page, "Blacksmith");
  await expect(page.locator("#research-iron-casting")).toBeDisabled();
  await page.locator("#research-forging").click();
  await expect(page.locator("#research-iron-casting")).toBeEnabled({
    timeout: 20_000,
  });
  await select(page, "Villager");
  await page.locator("#build-menu").click();
  await expect(page.locator("#build-monastery")).toBeEnabled();
  await expect(page.locator("#build-university")).toBeEnabled();
  await page.keyboard.press("v");
  await expect(page.locator("#build-castle")).toBeEnabled();
  await expect(page.locator("#build-siege-workshop")).toBeEnabled();
});
