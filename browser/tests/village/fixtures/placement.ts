import { expect, type Page } from "@playwright/test";
import { freshHud } from "./resources.js";

export async function arm(page: Page) {
  await page.goto("/?play=1#x=8192&y=8192&zoom=1");
  await expect(page.locator("#population")).toHaveText("6/10");
  const actors = JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as { kind: string; x: number; y: number }[];
  const worker = actors.find((a) => a.kind === "Villager");
  if (!worker) throw new Error("Missing fixture villager");
  await page.mouse.click(worker.x, worker.y);
  await page.locator("#build-menu").click();
  await page.locator("#build-house").click();
  await page.mouse.move(1000, 240);
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-placement-valid",
    "true",
  );
  await freshHud(page);
}
