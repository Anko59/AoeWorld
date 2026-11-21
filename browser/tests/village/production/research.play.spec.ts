import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { openTab } from "../../rendering/fixtures/cockpit-tabs.js";
import { productionFixture } from "./fixture.js";

test("Loom explains its effect, cost and time in both research panels", async ({
  page,
}) => {
  await productionFixture(page, [{ id: 40, kind: 0, x: 8187, y: 8190 }]);
  await page.keyboard.press("h");
  const loom = page.locator("#research-loom");
  await expect(loom).toBeVisible();
  const detail = /Villagers: \+15 HP.*50 gold.*25 s/;
  await loom.hover();
  await expect(page.getByRole("tooltip")).toHaveText(detail);

  await page.evaluate(() => {
    location.hash = "x=8192.5&y=8192.5&zoom=.04";
  });
  await expect(page.locator("#scene")).toHaveAttribute("data-lod", "markers");
  await openTab(page, "research");
  const townCenter = page.getByRole("group", { name: "Town Center research" });
  const farLoom = townCenter.getByRole("button", { name: "Loom" });
  await farLoom.focus();
  await expect(page.getByRole("tooltip")).toHaveText(detail);
});

test("the original DE pack resolves every research and unit portrait", async ({
  page,
  request,
}) => {
  const manifest = await request.get("/asset-pack/buildings/interface.json");
  test.skip(
    !manifest.ok(),
    "Requires the privately imported DE interface pack",
  );
  await page.goto("/?play=1&skipIntro=1");

  const icons = await page.evaluate(async () => {
    // @ts-expect-error Served browser module.
    const art = await import("/interface/ui-art.js");
    await art.artReady;
    const { rules } = (await (await fetch("/pkg/content.json")).json()) as {
      rules: number[];
    };
    const native = (html: string) => {
      const template = document.createElement("template");
      template.innerHTML = html;
      return template.content.querySelector(".original-icon image") !== null;
    };
    return {
      expected: [rules[2], rules[3]],
      actual: [art.techIds.length, art.trainingKinds.length],
      missingTechs: art.techIds.filter(
        (id: number) => !art.iconSprite("technologies", id),
      ),
      missingUnits: art.trainingKinds.filter(
        (kind: string) => !art.kindSprite(kind),
      ),
      fishTrap: native(art.entityIcon("FishTrap")),
    };
  });
  expect(icons.actual).toEqual(icons.expected);
  expect(icons.missingTechs).toEqual([]);
  expect(icons.missingUnits).toEqual([]);
  expect(icons.fishTrap).toBe(true);
});
