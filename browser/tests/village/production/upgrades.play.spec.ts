import { expect, type Page } from "@playwright/test";
import { test } from "../../../server.js";
import { playerState, productionFixture } from "./fixture.js";

async function select(page: Page, kind: string) {
  const actor = await page.locator("#scene").evaluate(
    (canvas, kind) =>
      (
        JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as {
          kind: string;
          x: number;
          y: number;
        }[]
      ).find((a) => a.kind === kind),
    kind,
  );
  if (!actor) throw new Error(`Missing ${kind}`);
  await page.mouse.click(actor.x, actor.y);
  await expect(page.locator("#selection")).toHaveAttribute("data-kind", kind);
}

test.setTimeout(90_000);

test("unit and technology successors replace their predecessors in the same slot", async ({
  page,
}) => {
  const { update } = await productionFixture(
    page,
    [
      { id: 40, kind: 6, x: 8186, y: 8190 },
      { id: 41, kind: 10, x: 8195, y: 8187, queue: Array<number>(15).fill(4) },
      { id: 42, kind: 2, x: 8188, y: 8196 },
      { id: 43, kind: 11, x: 8190, y: 8187 },
      { id: 44, kind: 9, x: 8185, y: 8196 },
    ],
    1,
    2 ** 2,
  );
  await select(page, "Barracks");
  const infantry = page.locator("#train-villager");
  await expect(infantry).toHaveAttribute("title", /Train Militia/);
  const militia = await infantry.boundingBox();
  const upgrade = await page.locator("#research-man-at-arms").boundingBox();
  await expect(page.locator("#research-long-swordsman")).toBeHidden();
  let mask = 2 ** 2 + 2 ** 8;
  update(playerState(1, mask));
  await expect(infantry).toHaveAttribute("title", /Train Man-at-Arms/);
  expect(await infantry.boundingBox()).toEqual(militia);
  await expect(page.locator("#research-man-at-arms")).toBeHidden();
  await expect(page.locator("#research-long-swordsman")).toBeVisible();
  expect(await page.locator("#research-long-swordsman").boundingBox()).toEqual(
    upgrade,
  );
  mask += 2 ** 14 + 2 ** 15;
  update(playerState(2, mask));
  await expect(infantry).toHaveAttribute("title", /Train Long Swordsman/);
  expect(await infantry.boundingBox()).toEqual(militia);
  await expect(page.locator("#research-long-swordsman")).toBeHidden();
  for (const [site, lesser, greater, tech] of [
    ["ArcheryRange", "archer", "crossbowman", 17],
    ["Stable", "scout", "light-cavalry", 19],
    ["Dock", "galley", "war-galley", 33],
  ] as const) {
    await select(page, site);
    await expect(page.locator(`#train-${lesser}`)).toBeVisible();
    await expect(page.locator(`#train-${greater}`)).toBeHidden();
    const slot = await page.locator(`#train-${lesser}`).boundingBox();
    const sibling =
      site === "ArcheryRange"
        ? await page.locator("#research-elite-skirmisher").boundingBox()
        : null;
    mask += 2 ** tech;
    update(playerState(2, mask));
    await expect(page.locator(`#train-${lesser}`)).toBeHidden();
    await expect(page.locator(`#train-${greater}`)).toBeVisible();
    expect(await page.locator(`#train-${greater}`).boundingBox()).toEqual(slot);
    if (sibling)
      expect(
        await page.locator("#research-elite-skirmisher").boundingBox(),
      ).toEqual(sibling);
  }
  await select(page, "LumberCamp");
  const axe = await page.locator("#research-double-bit-axe").boundingBox();
  await expect(page.locator("#research-bow-saw")).toBeHidden();
  mask += 2 ** 3;
  update(playerState(2, mask));
  await expect(page.locator("#research-double-bit-axe")).toBeHidden();
  await expect(page.locator("#research-bow-saw")).toBeVisible();
  expect(await page.locator("#research-bow-saw").boundingBox()).toEqual(axe);
  mask += 2 ** 28;
  update(playerState(2, mask));
  await expect(page.locator("#research-bow-saw")).toBeHidden();
});

test("Castle Age replaces Feudal Age without covering its earlier button", async ({
  page,
}) => {
  const { update } = await productionFixture(page, [
    { id: 40, kind: 0, x: 8187, y: 8190 },
  ]);
  await select(page, "TownCenter");
  const feudal = page.locator("#research-feudal-age");
  const castle = page.locator("#research-castle-age");
  await expect(feudal).toBeVisible();
  await expect(castle).toBeHidden();
  const slot = await feudal.boundingBox();
  update(playerState(1, 2 ** 2));
  await expect(feudal).toBeHidden();
  await expect(castle).toBeVisible();
  expect(await castle.boundingBox()).toEqual(slot);
  update(playerState(2, 2 ** 2 + 2 ** 14));
  await expect(castle).toBeHidden();
});
