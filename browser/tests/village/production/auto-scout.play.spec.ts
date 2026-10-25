import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { centerVillage, freshHud } from "../fixtures/resources.js";
import { automationBase, captureAutomation } from "./capture.js";

test("scout auto-explore toggles authoritative state and Stop cancels it", async ({
  page,
  request,
}) => {
  test.setTimeout(90_000);
  const base = await automationBase();
  expect((await request.post(`${base}maps/reset`)).status()).toBe(204);
  await gameAssets(page);
  await page.goto(`${base}?play=1`);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await centerVillage(page);
  const actors = JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as { kind: string; x: number; y: number }[];
  const worker = actors.find((a) => a.kind === "Villager");
  const scout = actors.find((a) => a.kind === "Scout");
  if (!worker || !scout) throw new Error("Starting actors missing");
  await page.mouse.click(worker.x, worker.y);
  await expect(page.locator("#auto-scout")).toBeHidden();
  await page.mouse.click(scout.x, scout.y);
  await expect(page.locator("#auto-scout")).toBeVisible();
  await expect(page.locator("#auto-scout")).toHaveAttribute(
    "aria-pressed",
    "false",
  );
  await page.locator("#auto-scout").click();
  await expect(page.locator("#auto-scout")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await captureAutomation(page, base, `auto-scout-${test.info().project.name}`);
  await expect
    .poll(
      async () => {
        await freshHud(page);
        const next = JSON.parse(
          (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
        ) as { kind: string; x: number; y: number }[];
        const at = next.find((a) => a.kind === "Scout");
        return at ? Math.hypot(at.x - scout.x, at.y - scout.y) : 0;
      },
      { timeout: 20_000 },
    )
    .toBeGreaterThan(15);
  await page.locator("#auto-scout").click();
  await expect(page.locator("#auto-scout")).toHaveAttribute(
    "aria-pressed",
    "false",
  );
  await page.locator("#auto-scout").click();
  await expect(page.locator("#auto-scout")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await page.locator("#stop-units").click();
  await expect(page.locator("#auto-scout")).toHaveAttribute(
    "aria-pressed",
    "false",
  );
  await expect(page.locator("#selection-task")).toHaveText("Idle");
});
