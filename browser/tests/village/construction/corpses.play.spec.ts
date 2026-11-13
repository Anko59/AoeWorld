import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import {
  centerVillage,
  freshHud,
  scoutVillage,
} from "../fixtures/resources.js";

test.use({ video: "on" });

test("a House completes over a stationary dying villager", async ({
  page,
  request,
}) => {
  test.setTimeout(150_000);
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
  await page.goto("/?play=1#zoom=.45");
  await expect(page.locator("#connection")).toHaveText("connected");
  await scoutVillage(page, 9, 3);
  await page.evaluate(() => {
    location.hash = "zoom=.3";
  });
  await centerVillage(page);
  await freshHud(page);
  type Actor = {
    id: number;
    kind: string;
    x: number;
    y: number;
    hp: number;
    task: string;
  };
  const actors = async () =>
    JSON.parse(
      (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
    ) as Actor[];
  let start = await actors();
  let boar = start.find((a) => a.kind === "Boar");
  if (!boar) throw new Error("Starting boar missing");
  // Natural starts put the herd in a seeded direction. Zoom out until it sits
  // clear of the bottom HUD panel (below y=512) so the order reaches the boar.
  const size = page.viewportSize();
  if (!size) throw new Error("Viewport missing");
  const dy = boar.y - size.height / 2;
  const fit = Math.min(
    1,
    (dy > 0 ? 120 : 280) / Math.max(1, Math.abs(dy)),
    (size.width / 2 - 40) / Math.max(1, Math.abs(boar.x - size.width / 2)),
  );
  if (fit < 1) {
    await page.evaluate(
      (zoom) => {
        location.hash = `zoom=${zoom}`;
      },
      (0.3 * fit * 0.9).toFixed(3),
    );
    await centerVillage(page);
    await freshHud(page);
    start = await actors();
    boar = start.find((a) => a.id === boar?.id);
    if (!boar) throw new Error("Starting boar missing after zoom");
  }
  const hunter = start
    .filter((a) => a.kind === "Villager")
    .sort(
      (a, b) =>
        Math.hypot(a.x - boar.x, a.y - boar.y) -
        Math.hypot(b.x - boar.x, b.y - boar.y),
    )[0];
  if (!hunter) throw new Error("Starting hunter missing");
  await page.mouse.click(hunter.x, hunter.y);
  await freshHud(page);
  await page.mouse.click(boar.x, boar.y, { button: "right" });
  await expect
    .poll(async () => (await actors()).find((a) => a.id === hunter.id)?.hp, {
      timeout: 70_000,
    })
    .toBe(0);
  await freshHud(page);
  const corpse = (await actors()).find((a) => a.id === hunter.id);
  if (!corpse) throw new Error("Corpse missing");
  const builder = (await actors())
    .filter((a) => a.kind === "Villager" && a.hp > 0)
    .sort(
      (a, b) =>
        Math.hypot(a.x - corpse.x, a.y - corpse.y) -
        Math.hypot(b.x - corpse.x, b.y - corpse.y),
    )[0];
  if (!builder) throw new Error("Living builder missing");
  expect(corpse.task).toBe("Dying");
  await page.mouse.click(builder.x, builder.y);
  await freshHud(page);
  await expect(page.locator("#selection-name")).toHaveText("Villager");
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "1");
  await page.locator("#build-menu").click();
  await page.locator("#build-house").click();
  // Placement uses the pointed tile as the footprint origin, so the dead
  // villager's tile lies inside this two-by-two foundation.
  await page.mouse.move(corpse.x, corpse.y);
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-placement-valid",
    "true",
  );
  await page.mouse.click(corpse.x, corpse.y);
  await expect
    .poll(async () => (await actors()).some((a) => a.kind === "House"))
    .toBe(true);
  await freshHud(page);
  const dying = (await actors()).find((a) => a.id === hunter.id);
  expect(dying).toMatchObject({
    hp: 0,
    task: "Dying",
    x: corpse.x,
    y: corpse.y,
  });
  const house = (await actors()).find((a) => a.kind === "House");
  if (!house) throw new Error("House foundation missing");
  await page.mouse.click(house.x, house.y);
  await expect(page.locator("#selection-name")).toHaveText("House");
  await expect(page.locator("#building-status")).toContainText("Complete", {
    timeout: 45_000,
  });
  await expect(page.locator("#building-hp")).toHaveText("550 / 550 HP");
});
