import { expect, type Page } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { centerVillage, freshHud } from "./fixtures/resources.js";

type Actor = { id: number; kind: string; x: number; y: number; hp: number };
const actors = async (page: Page): Promise<Actor[]> =>
  JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as Actor[];

test("the Delete key kills a selected villager through the death path", async ({
  page,
  request,
}) => {
  test.setTimeout(120_000);
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
  await page.goto("/?play=1#zoom=.45");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await centerVillage(page);
  await freshHud(page);
  const villager = (await actors(page)).find(
    (a) => a.kind === "Villager" && a.hp > 0,
  );
  if (!villager) throw new Error("Villager missing from the authoritative HUD");
  await page.mouse.click(villager.x, villager.y);
  await expect(page.locator("#selection-name")).toHaveText("Villager");
  await expect(page.locator("#delete-units")).toHaveAttribute(
    "title",
    "Delete (Del)",
  );
  await page.keyboard.press("Delete");
  // A dead body may leave the authoritative list; either way it is not alive.
  await expect
    .poll(
      async () =>
        (await actors(page)).find((a) => a.id === villager.id)?.hp ?? 0,
    )
    .toBe(0);
});

test("the Delete button cancels a selected House foundation and refunds its wood", async ({
  page,
  request,
}) => {
  test.setTimeout(120_000);
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
  await page.goto("/?play=1#zoom=.45");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await centerVillage(page);
  await freshHud(page);
  const worker = (await actors(page)).find(
    (a) => a.kind === "Villager" && a.hp > 0,
  );
  if (!worker) throw new Error("Villager missing from the authoritative HUD");
  await page.mouse.click(worker.x, worker.y);
  await page.locator("#build-menu").click();
  await page.locator("#build-house").click();
  let placed = false;
  for (const [x, y] of [
    [420, 320],
    [850, 320],
    [420, 420],
    [850, 420],
    [640, 220],
  ] as const) {
    await page.mouse.move(x, y);
    await page.waitForTimeout(300);
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
  await expect(page.locator("#stockpile-wood")).toHaveText("175");
  await expect
    .poll(async () => (await actors(page)).some((a) => a.kind === "House"))
    .toBe(true);
  const house = (await actors(page)).find((a) => a.kind === "House");
  if (!house) throw new Error("House foundation missing");
  await page.mouse.click(house.x, house.y);
  await expect(page.locator("#selection-name")).toHaveText("House");
  await expect(page.locator("#delete-units")).toBeVisible();
  await page.locator("#delete-units").click();
  await expect
    .poll(async () => (await actors(page)).some((a) => a.kind === "House"))
    .toBe(false);
  await expect(page.locator("#stockpile-wood")).toHaveText("200");
  await freshHud(page);
});
