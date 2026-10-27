import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { centerVillage, freshHud } from "../fixtures/resources.js";
import { automationBase, captureAutomation } from "./capture.js";

test("Mill reseeds charge wood, persist after reload, and refund cancellation", async ({
  page,
  request,
}) => {
  test.setTimeout(140_000);
  const base = await automationBase();
  expect((await request.post(`${base}maps/reset`)).status()).toBe(204);
  await gameAssets(page);
  await page.goto(`${base}?play=1`);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await centerVillage(page);
  const actors = async () =>
    JSON.parse(
      (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
    ) as { kind: string; x: number; y: number }[];
  const worker = (await actors()).find((a) => a.kind === "Villager");
  if (!worker) throw new Error("Starting villager missing");
  await page.mouse.click(worker.x, worker.y);
  await page.locator("#build-menu").click();
  await page.locator("#build-mill").click();
  await page.mouse.move(900, 380);
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-placement-valid",
    "true",
  );
  await page.mouse.click(900, 380);
  await expect
    .poll(async () => (await actors()).some((a) => a.kind === "Mill"))
    .toBe(true);
  const selectMill = async () => {
    await freshHud(page);
    const mill = (await actors()).find((a) => a.kind === "Mill");
    if (!mill) throw new Error("Mill missing");
    await page.mouse.click(mill.x, mill.y);
    await expect(page.locator("#selection-name")).toHaveText("Mill");
  };
  await selectMill();
  await expect(page.locator("#building-status")).toContainText("Complete", {
    timeout: 70_000,
  });
  await expect(page.locator("#farm-reseed-count")).toHaveText("0");
  await page.locator("#add-farm-reseed").click();
  await expect(page.locator("#farm-reseed-count")).toHaveText("1");
  await expect(page.locator("#stockpile-wood")).toHaveText("40");
  await captureAutomation(
    page,
    base,
    `mill-reseeds-${test.info().project.name}`,
  );
  await expect(page.locator("#add-farm-reseed")).toContainText("1");
  await page.reload();
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await centerVillage(page);
  await selectMill();
  await expect(page.locator("#farm-reseed-count")).toHaveText("1");
  await page.locator("#add-farm-reseed").click({ button: "right" });
  await expect(page.locator("#farm-reseed-count")).toHaveText("0");
  await expect(page.locator("#stockpile-wood")).toHaveText("100");
});
