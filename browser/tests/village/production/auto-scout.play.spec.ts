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
  await expect(page.locator("#auto-scout")).toHaveCSS("outline-width", "2px");
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

test("one order scouts and stops thirty selected cavalry", async ({
  page,
  request,
}) => {
  test.setTimeout(90_000);
  const base = await automationBase();
  expect(
    (
      await request.post(`${base}maps/reset?scenario=selection-orders`)
    ).status(),
  ).toBe(204);
  await gameAssets(page);
  await page.goto(`${base}?play=1#x=64&y=71&zoom=.5`);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await expect(page.locator("#minimap")).toHaveAttribute("data-camera-y", "71");
  await freshHud(page);
  const actors = JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as { kind: string; x: number; y: number }[];
  const cavalry = actors.find((actor) => actor.kind === "LightCavalry");
  if (!cavalry) throw new Error("Starting cavalry missing");
  await page.mouse.dblclick(cavalry.x, cavalry.y);
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "30");
  const autoScout = page.locator("#auto-scout");
  await expect(autoScout).toHaveAttribute("aria-pressed", "false");
  await autoScout.click();
  await expect(autoScout).toHaveAttribute("aria-pressed", "true");
  await page.locator("#stop-units").click();
  await expect(autoScout).toHaveAttribute("aria-pressed", "false");
});
