import { expect, type Page } from "@playwright/test";
import { fallbackTest as test, forceCanvas } from "../../rendering/backends.js";
import { gameAssets } from "../../game-assets.js";
import { centerVillage, freshHud } from "../fixtures/resources.js";
import { observeOrders, required, waitForGround } from "./roster-orders.js";

type Actor = {
  id: number;
  kind: string;
  owned: boolean;
  selected: boolean;
  x: number;
  y: number;
  hp: number;
  task: string;
};
const sites = [
  "TownCenter",
  "Barracks",
  "ArcheryRange",
  "Stable",
  "SiegeWorkshop",
  "Castle",
  "Monastery",
  "Market",
  "Dock",
];

async function actors(page: Page): Promise<Actor[]> {
  return JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as Actor[];
}

async function centerCoast(page: Page) {
  await page.locator("#minimap").evaluate((map) => {
    const x = Number(map.getAttribute("data-camera-x"));
    const y = Number(map.getAttribute("data-camera-y"));
    // The coastal fixture places the Dock southeast of the Town Center.
    location.hash = `x=${x + 13}&y=${y - 3}&zoom=.4`;
  });
  await expect
    .poll(async () => {
      const dock = (await actors(page)).find((a) => a.kind === "Dock");
      return dock ? Math.abs(dock.x - 800) : Infinity;
    })
    .toBeLessThan(50);
  await freshHud(page);
}

async function select(page: Page, actor: Actor) {
  await expect(async () => {
    const current = (await actors(page)).find((a) => a.id === actor.id);
    if (!current)
      throw new Error(`${actor.kind} ${actor.id} is outside the view`);
    await page.mouse.click(current.x, current.y);
    await expect
      .poll(async () => (await actors(page)).find((a) => a.selected)?.id, {
        timeout: 1_000,
        message: `select ${actor.kind} ${actor.id}`,
      })
      .toBe(actor.id);
  }).toPass({ timeout: 10_000 });
}

async function firstCommandPage(page: Page) {
  const previous = page.locator("#previous-commands");
  while (await previous.isVisible()) await previous.click();
}

async function trainingOptions(page: Page) {
  await firstCommandPage(page);
  const buttons = page.locator(
    '#command-grid button[id^="train-"]:visible:enabled',
  );
  const currentPage = () =>
    buttons.evaluateAll((nodes) =>
      nodes.map((node) => ({
        option: node.id,
        title: node.getAttribute("title"),
        portraits: Array.from(
          node.querySelectorAll('[data-ui-group="units"]'),
          (icon) => icon.getAttribute("data-ui-id"),
        ),
      })),
    );
  const options = await currentPage();
  const next = page.locator("#more-commands");
  while (await next.isVisible()) {
    await next.click();
    options.push(...(await currentPage()));
  }
  return options;
}

async function trainingButton(page: Page, id: string) {
  await firstCommandPage(page);
  const button = page.locator(`#${id}`);
  const next = page.locator("#more-commands");
  while (!(await button.isVisible()) && (await next.isVisible()))
    await next.click();
  return button;
}

test.use({ viewport: { width: 1600, height: 900 } });

test("the live browser trains the data roster and orders every trained unit", async ({
  page,
  request,
}) => {
  test.setTimeout(480_000);
  const command = await observeOrders(page);
  const trained = new Set<number>();
  const nativeUnitName = new Map([
    ["Scout", "Scout Cavalry"],
    ["ManAtArms", "Man-at-Arms"],
    ["TwoHandedSwordsman", "Two-Handed Swordsman"],
  ]);
  const buttonsSeen = new Set<string>();
  await forceCanvas(page);
  const nativeArt = (
    await request.get("/asset-pack/buildings/interface.json")
  ).ok();
  await gameAssets(page, true);
  const response = await request.get("/pkg/content.json");
  expect(response.ok()).toBe(true);
  const { rules } = (await response.json()) as { rules: number[] };
  const rule = (index: number) => required(rules[index], `rule ${index}`);
  const [buildingCount, techCount, unitCount] = [rule(1), rule(2), rule(3)];
  const unitsAt = 5 + buildingCount * 15 + techCount * 12;
  const combat = Array.from({ length: unitCount }, (_, i) =>
    rule(unitsAt + i * 22 + 2),
  );
  // The production associations, rather than combat stats, say what can train.
  let trainable = 0n;
  for (let i = 0; i < buildingCount; i++) {
    const at = 5 + i * 15;
    trainable |=
      BigInt(rule(at + 10) >>> 0) | (BigInt(rule(at + 11) >>> 0) << 32n);
  }

  for (const stage of [0, 1, 2, 3, 4]) {
    await test.step(`production tier ${stage}`, async () => {
      expect(
        (
          await request.post(
            `/maps/reset?scenario=content-roster&stage=${stage}`,
          )
        ).status(),
      ).toBe(204);
      await page.goto(`/?play=1&rosterStage=${stage}#zoom=.4`);
      await expect(page.locator("#connection")).toHaveText("connected", {
        timeout: 60_000,
      });
      await centerVillage(page);
      await expect
        .poll(
          async () =>
            (await actors(page)).filter(
              (a) => a.owned && sites.includes(a.kind),
            ).length,
        )
        .toBe(sites.length);
      if (stage === 0) {
        const menu = await page.evaluate(() => {
          const buildings = Reflect.get(window, "aoeBuildings") as {
            techs: { art: number }[];
          };
          return buildings.techs.map((tech) => tech.art);
        });
        expect(menu).toHaveLength(techCount);
        expect(new Set(menu).size).toBe(techCount);
        if (nativeArt) {
          const icons = await request.get(
            "/asset-pack/buildings/interface.json",
          );
          expect(icons.ok()).toBe(true);
          const art = (await icons.json()) as {
            technologies: Record<string, unknown>;
          };
          for (const id of menu)
            expect(art.technologies[id], `research icon ${id}`).toBeTruthy();
        }
      }

      for (const site of sites) {
        if (site === "Dock") await centerCoast(page);
        const building = required(
          (await actors(page)).find((a) => a.owned && a.kind === site),
          site,
        );
        await select(page, building);
        const options = await trainingOptions(page);
        for (const { option, title, portraits } of options) {
          expect(portraits, option).toHaveLength(1);
          const portrait = required(portraits[0], `portrait for ${option}`);
          const buttonKey = `${site}/${option}/${portrait}`;
          if (buttonsSeen.has(buttonKey)) continue;
          buttonsSeen.add(buttonKey);
          await select(page, building);
          const button = await trainingButton(page, option);
          const before = new Set((await actors(page)).map((a) => a.id));
          const kind = await command(5, building.id, () => button.click());
          let born: Actor | undefined;
          await expect
            .poll(
              async () => {
                born = (await actors(page)).find(
                  (a) => a.owned && !before.has(a.id),
                );
                return born?.kind;
              },
              { message: `unit from ${option}`, timeout: 20_000 },
            )
            .toBeTruthy();
          const unit = required(born, `unit from ${option}`);
          await select(page, unit);
          const name =
            nativeUnitName.get(unit.kind) ??
            unit.kind.replace(/([a-z])([A-Z])/g, "$1 $2");
          expect(title).toContain(`Train ${name}`);
          await Promise.all([
            expect(page.locator("#selection")).toHaveAttribute(
              "data-kind",
              unit.kind,
            ),
            expect(page.locator("#selection-name")).toHaveText(name),
            expect(
              page.locator("#selection-portrait [data-ui-id]"),
            ).toHaveAttribute("data-ui-id", portrait),
            expect(page.locator("#move-units")).toBeVisible(),
            expect(page.locator("#stop-units")).toBeVisible(),
          ]);
          const attackMove = page.locator("#attack-move-units");
          const patrol = page.locator("#patrol-units");
          if ((combat[kind] ?? 0) > 0) {
            await expect(attackMove, name).toBeVisible();
            await expect(
              attackMove.locator('[data-ui-group="commands"]'),
            ).toHaveAttribute("data-ui-id", "attack-move");
            if (unit.kind !== "Villager" && unit.kind !== "General")
              await expect(patrol, name).toBeVisible();
          } else {
            await expect(attackMove, name).toBeHidden();
            await expect(patrol, name).toBeHidden();
          }
          if (trained.has(kind)) continue;

          const stance = page.locator("#stance-no-attack");
          if (await stance.isVisible()) await stance.click();
          const naval = site === "Dock";
          // The fixture clears an open tile west of town and another east
          // of the Dock. Project each from its known building centre.
          const landmarkKind = naval ? "Dock" : "TownCenter";
          const landmark = required(
            (await actors(page)).find(
              (a) => a.owned && a.kind === landmarkKind,
            ),
            landmarkKind,
          );
          const zoom = Number(
            await page.locator("#minimap").getAttribute("data-zoom"),
          );
          const [dx, dy] = naval ? [7.5, -1] : [-12.5, -4.5];
          const destination = {
            x: landmark.x + (dx - dy) * 64 * zoom,
            y: landmark.y + (dx + dy) * 32 * zoom,
          };
          const moveStart = required(
            (await actors(page)).find((a) => a.id === unit.id),
            name,
          );
          const general = unit.kind === "General";
          await waitForGround(page, destination);
          await page
            .locator(
              general
                ? "#general-only-move"
                : unit.kind === "Galley"
                  ? "#attack-move-units"
                  : "#move-units",
            )
            .click();
          await command(general ? 32 : 2, unit.id, () =>
            page.mouse.click(destination.x, destination.y, {
              button: unit.kind === "Galley" ? "right" : "left",
            }),
          );
          await expect
            .poll(async () => {
              const current = (await actors(page)).find(
                (a) => a.id === unit.id,
              );
              return current
                ? Math.hypot(current.x - moveStart.x, current.y - moveStart.y)
                : 0;
            })
            .toBeGreaterThan(10);

          const target = (await actors(page)).find(
            (a) =>
              !a.owned &&
              a.hp > 0 &&
              a.kind ===
                (naval
                  ? "TransportShip"
                  : unit.kind === "Monk"
                    ? "TradeCart"
                    : "House"),
          );
          if (!target) throw new Error(`No enemy target in view for ${name}`);
          // A Monk's attack order starts conversion. Unarmed units still send
          // the order through the normal UI and the server refuses it.
          const arms = (combat[kind] ?? 0) > 0 || unit.kind === "Monk";
          const attack = await command(
            7,
            unit.id,
            () => page.mouse.click(target.x, target.y, { button: "right" }),
            arms ? 0 : 10,
          );
          expect(attack).toBe(target.id);
          if (arms) {
            await expect
              .poll(
                async () => {
                  const visible = await actors(page);
                  const enemy = visible.find((a) => a.id === target.id);
                  if (unit.kind !== "Monk")
                    return enemy !== undefined && enemy.hp < target.hp;
                  // The converted unit replaces the old identity in place.
                  return (
                    enemy === undefined &&
                    visible.some(
                      (a) =>
                        a.owned &&
                        a.kind === target.kind &&
                        Math.hypot(a.x - target.x, a.y - target.y) < 2,
                    )
                  );
                },
                {
                  message: `${name} lands its attack or conversion`,
                  timeout: 20_000,
                },
              )
              .toBe(true);
          }
          if (unit.kind === "Trebuchet") {
            await expect(page.locator("#selection")).toHaveAttribute(
              "data-kind",
              "TrebuchetUnpacked",
            );
            await expect(page.locator("#selection-name")).toHaveText(
              "Trebuchet (unpacked)",
            );
            await page.locator("#move-units").click();
            await command(2, unit.id, () => page.mouse.click(unit.x, unit.y));
            await expect(page.locator("#selection-name")).toHaveText(
              "Trebuchet",
            );
          }
          trained.add(kind);
          if ((await actors(page)).some((a) => a.id === unit.id && a.hp > 0)) {
            await page.locator("#delete-units").click();
            await expect
              .poll(async () =>
                (await actors(page)).some((a) => a.id === unit.id && a.hp > 0),
              )
              .toBe(false);
          }
        }
      }
    });
  }
  expect([...trained].sort((a, b) => a - b)).toEqual(
    combat.flatMap((_, i) =>
      (trainable & (1n << BigInt(i))) !== 0n ? [i] : [],
    ),
  );
});

test("a Fishing Ship builds and works a Fish Trap in the Imperial Age", async ({
  page,
  request,
}) => {
  test.setTimeout(120_000);
  await forceCanvas(page);
  await gameAssets(page, true);
  const command = await observeOrders(page);
  expect(
    (await request.post("/maps/reset?scenario=content-roster")).status(),
  ).toBe(204);
  await page.goto("/?play=1#zoom=.4");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await centerVillage(page);
  await centerCoast(page);
  await expect
    .poll(async () =>
      (await actors(page)).some((a) => a.owned && a.kind === "Dock"),
    )
    .toBe(true);
  const dock = required(
    (await actors(page)).find((a) => a.kind === "Dock"),
    "Dock",
  );
  await select(page, dock);
  expect(
    await command(5, dock.id, () => page.locator("#train-villager").click()),
  ).toBe(3);
  await expect
    .poll(async () =>
      (await actors(page)).some((a) => a.owned && a.kind === "FishingShip"),
    )
    .toBe(true);
  const ship = required(
    (await actors(page)).find((a) => a.kind === "FishingShip"),
    "Fishing Ship",
  );
  await select(page, ship);
  await page.locator("#build-fish-trap").click();
  const scene = page.locator("#scene");
  const water = { x: ship.x + 128, y: ship.y + 64 };
  await page.mouse.move(water.x, water.y);
  await expect(scene).toHaveAttribute("data-placement-valid", "true");
  await page.mouse.click(water.x, water.y);
  let trap: Actor | undefined;
  await expect
    .poll(async () => {
      trap = (await actors(page)).find(
        (a) => a.owned && a.kind === "FishTrap" && a.hp >= 250,
      );
      return trap?.id;
    })
    .toBeTruthy();
  const completed = required(trap, "completed Fish Trap");
  // The Fishing Ship is on top of the completed trap. Clear the trap's
  // footprint through a normal move so the building underneath can be picked.
  await page.locator("#move-units").click();
  await command(2, ship.id, () =>
    page.mouse.click(completed.x + 72, completed.y - 36),
  );
  await expect
    .poll(async () => {
      const vessel = (await actors(page)).find((a) => a.id === ship.id);
      return vessel
        ? Math.hypot(vessel.x - completed.x, vessel.y - completed.y)
        : 0;
    })
    .toBeGreaterThan(40);
  await select(page, completed);
  await expect(page.locator("#selection-name")).toHaveText("Fish Trap");
  await expect(page.locator("#building-status")).toContainText(
    /\d+ food remaining/,
  );
  await select(
    page,
    required(
      (await actors(page)).find((a) => a.id === ship.id),
      "Fishing Ship",
    ),
  );
  await page.mouse.click(completed.x, completed.y, { button: "right" });
  await expect
    .poll(async () => (await actors(page)).find((a) => a.id === ship.id)?.task)
    .toMatch(/Fishing|Returning|Carrying/);
  await expect(page.locator("#workers-food")).toHaveText("1");
});
