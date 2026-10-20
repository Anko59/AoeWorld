import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { fiveVillagerFixture } from "./fixture.js";
import { freshHud } from "./fixtures/resources.js";

test.use({
  viewport: { width: 1280, height: 720 },
  deviceScaleFactor: 2,
});

test("double click excludes villagers hidden by the top bar on a high-DPI canvas", async ({
  page,
}) => {
  await gameAssets(page, true);
  await fiveVillagerFixture(page);
  await page.goto("/#x=8202.8125&y=8202.8125&zoom=1");
  await expect(page.locator("#population")).toHaveText("6/10");
  await freshHud(page);
  const actors = await page.locator("#scene").evaluate((canvas) => {
    const c = canvas as HTMLCanvasElement;
    if (c.height / c.clientHeight !== 2)
      throw new Error("Expected a double-density backing");
    return (
      JSON.parse(c.getAttribute("data-actors") ?? "[]") as {
        kind: string;
        x: number;
        y: number;
      }[]
    ).filter((a) => a.kind === "Villager");
  });
  expect(actors).toHaveLength(5);
  const visible = actors.filter((a) => a.y >= 54 && a.y < 512);
  // Three ground anchors, and all their art, are behind the top HTML bar.
  expect(actors.filter((a) => a.y < 54)).toHaveLength(3);
  expect(visible).toHaveLength(2);
  const first = visible[0];
  if (!first) throw new Error("Visible fixture villager missing");
  await page.mouse.dblclick(first.x, first.y);
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "2");
  await expect(page.locator("#selected-icons .unit-token")).toHaveCount(2);
});
