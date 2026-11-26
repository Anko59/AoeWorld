import {
  expect,
  type Page,
  type APIRequestContext,
  type TestInfo,
} from "@playwright/test";
import { gameAssets } from "../../game-assets.js";
import { centerVillage, freshHud } from "../fixtures/resources.js";
import { forceCanvas } from "../../rendering/backends.js";

type Actor = {
  id: number;
  owned: boolean;
  kind: string;
  x: number;
  y: number;
  hp: number;
  maxHp: number;
  task: string;
};
async function actors(page: Page): Promise<Actor[]> {
  return JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as Actor[];
}
// Soldiers now engage on their own and move between reading a position and
// the click, so re-read and click until the selection takes.
async function select(page: Page, kind: string) {
  await expect(async () => {
    const actor = (await actors(page)).find(
      (a) => a.kind === kind && a.x > 0 && a.x < 1280 && a.y > 54 && a.y < 512,
    );
    if (!actor) throw new Error(`Visible ${kind} missing`);
    await page.mouse.click(actor.x, actor.y);
    await expect(page.locator("#selection")).toHaveAttribute(
      "data-kind",
      kind,
      {
        timeout: 1_000,
      },
    );
  }).toPass({ timeout: 15_000 });
}

export async function militaryAttack(
  page: Page,
  request: APIRequestContext,
  info: TestInfo,
) {
  info.setTimeout(90_000);
  if (info.project.name === "canvas") await forceCanvas(page);
  expect(
    (await request.post("/maps/reset?scenario=feudal-military")).status(),
  ).toBe(204);
  await gameAssets(page);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
  await centerVillage(page);
  await page.mouse.move(640, 360);
  await page.mouse.wheel(0, 288);
  await freshHud(page);
  await select(page, "ArcheryRange");
  await page.locator("#train-archer").click();
  await select(page, "Barracks");
  await page.locator("#train-spearman").click();
  await select(page, "Blacksmith");
  await page.locator("#research-fletching").click();
  await expect
    .poll(async () => (await actors(page)).some((a) => a.kind === "Archer"), {
      timeout: 20_000,
    })
    .toBe(true);
  await expect
    .poll(async () => (await actors(page)).some((a) => a.kind === "Spearman"))
    .toBe(true);
  // The scenario's enemy Militia and Scout now raid the village; the new
  // Spearman fights back without an order.
  const enemies = (await actors(page)).filter((a) => !a.owned);
  const militia = enemies.find((a) => a.kind === "Militia");
  const scout = enemies.find((a) => a.kind === "Scout");
  if (!militia || !scout) throw new Error("Enemy Militia or Scout missing");
  await expect
    .poll(async () => damaged(page, militia.id), {
      timeout: 40_000,
      message: "the idle Spearman fights back against the raiding Militia",
    })
    .toBe(true);
  await expect(page.locator("#research-fletching")).toBeDisabled();
  await select(page, "Archer");
  await expect(page.locator("#unit-combat")).toContainText("Attack 5");
  await expect(page.locator("#unit-combat")).toContainText("Range 5");
  // No Attack leaves the Archer only explicit orders, so an Attacking task
  // and damage to the target after the click both come from the order.
  await page.locator("#stance-no-attack").click();
  const archer = (await actors(page)).find(
    (a) => a.owned && a.kind === "Archer",
  );
  if (!archer) throw new Error("Archer missing");
  await expect
    .poll(async () => task(page, archer.id), { timeout: 10_000 })
    .not.toBe("Attacking");
  // The raid is a live fight, not a chase: the enemy Scout trades blows with the player's
  // own Scout and is often dead (CI traces: both Dying within seconds), while the Militia
  // keeps raiding. So order the attack on a living raider read just now, preferring the
  // Scout, and re-read before every click: a unit moves between a read and the click, and a
  // click that lands on the ground becomes a Move order, which never counts as Attacking.
  let ordered: Actor | undefined;
  await expect
    .poll(
      async () => {
        if ((await task(page, archer.id)) === "Attacking") return "Attacking";
        const raiders = (await actors(page)).filter(
          (a) => !a.owned && a.hp > 0 && a.task !== "Dying",
        );
        ordered = raiders.find((a) => a.id === scout.id) ?? raiders[0];
        if (!ordered) return "No living raider in view";
        await page.mouse.click(ordered.x, ordered.y, { button: "right" });
        // Wait for the Archer to take the order rather than for a fixed time.
        return expect
          .poll(() => task(page, archer.id), { timeout: 1_500 })
          .toBe("Attacking")
          .then(
            () => "Attacking",
            () => task(page, archer.id),
          );
      },
      {
        timeout: 15_000,
        message: "the No Attack Archer attacks only on the explicit order",
      },
    )
    .toBe("Attacking");
  await expect
    .poll(
      async () =>
        !(await actors(page)).some(
          (a) => a.id === ordered?.id && a.hp >= (ordered?.hp ?? 0),
        ),
      {
        timeout: 20_000,
        message:
          "the ordered Archer damages the enemy it was ordered to attack",
      },
    )
    .toBe(true);
}

async function task(page: Page, id: number) {
  return (await actors(page)).find((a) => a.id === id)?.task;
}

// Damaged, or killed and removed.
async function damaged(page: Page, id: number) {
  return !(await actors(page)).some((a) => a.id === id && a.hp === a.maxHp);
}
