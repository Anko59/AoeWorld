import { expect, type Page } from "@playwright/test";
import { fallbackTest as test } from "../../rendering/backends.js";
import { gameAssets } from "../../game-assets.js";
import { freshHud } from "../fixtures/resources.js";

type Actor = { kind: string; x: number; y: number; owned: boolean };

async function actor(page: Page, kind: string): Promise<Actor> {
  const read = async () =>
    (
      JSON.parse(
        (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
      ) as Actor[]
    ).find((entry) => entry.kind === kind);
  await expect.poll(read).toBeDefined();
  const found = await read();
  if (!found) throw new Error(`${kind} left the view`);
  return found;
}

test("a Monk collects a relic and a Monastery starts paying gold", async ({
  page,
  request,
}) => {
  await gameAssets(page, true);
  expect((await request.post("/maps/reset?scenario=relics")).ok()).toBe(true);
  await page.goto("/?play=1#x=66&y=66&zoom=.8");
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-connected",
    "true",
  );
  await freshHud(page);
  await expect(page.locator("#minimap-relics circle")).toHaveCount(1);
  const gold = page.locator("#stockpile-gold");
  const before = Number((await gold.textContent())?.replaceAll(",", ""));
  const relic = await actor(page, "Relic");
  await page.mouse.click(relic.x, relic.y);
  await expect(page.locator("#selection-name")).toHaveText("Relic");

  const monk = await actor(page, "Monk");
  await page.mouse.click(monk.x, monk.y);
  await page.mouse.click(relic.x, relic.y, { button: "right" });
  await expect(page.locator("#relic-status")).toHaveText("Carrying a relic");
  await expect(
    page.locator('#carried-loads [title="Carrying a relic"]'),
  ).toBeVisible();
  await expect(page.locator("#minimap-relics circle")).toHaveCount(0);
  await expect(gold).toHaveText(String(before));

  const monastery = await actor(page, "Monastery");
  await page.mouse.click(monastery.x, monastery.y, { button: "right" });
  await expect(page.locator("#relic-status")).toBeHidden();
  await page.mouse.click(monastery.x, monastery.y);
  await expect(page.locator("#relic-status")).toHaveText("1 relic stored");
  await expect
    .poll(async () => Number((await gold.textContent())?.replaceAll(",", "")))
    .toBeGreaterThan(before);
});
