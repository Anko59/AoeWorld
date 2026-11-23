import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { playerState, productionFixture } from "./fixture.js";
import { openTab } from "../../rendering/fixtures/cockpit-tabs.js";

test("the far roster lists only units an owned producer can train now", async ({ page }) => {
  const { update } = await productionFixture(page, [
    { id: 1, kind: 0, x: 8184, y: 8180 },
    { id: 2, kind: 6, x: 8191, y: 8186 },
  ]);
  update(playerState(0, 0, 4, 100));
  await page.evaluate(() => { location.hash = "x=8192.5&y=8192.5&zoom=.04"; });
  await expect(page.locator("#scene")).toHaveAttribute("data-lod", "markers");
  await openTab(page, "train");
  const grid = page.getByRole("group", { name: "Unit type" });
  const cell = (name: string) => grid.getByRole("button", { name, exact: true });
  await expect(cell("Militia")).toBeVisible();
  await expect(cell("Villager")).toBeVisible();
  for (const name of ["Archer", "Galley", "Fishing Ship", "Man-at-Arms"])
    await expect(cell(name)).toHaveCount(0);

  // Feudal Age and Man-at-Arms have been researched.
  update(playerState(1, (1 << 2) | (1 << 8), 4, 100));
  await expect(cell("Man-at-Arms")).toBeVisible();
  await expect(cell("Militia")).toHaveCount(0);
  await expect(cell("Villager")).toBeVisible();
});
