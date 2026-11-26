import { expect, type Page } from "@playwright/test";
import { test } from "../../../server.js";
import { shown } from "../../village/formations/minimap.js";
import { freshHud } from "../../village/fixtures/resources.js";
import { expectStars, stars } from "./alert-page.js";
import { battle, type Hit } from "./battle.js";

type Point = { x: number; y: number };

const home = "x=8192.5&y=8192.5&zoom=1";
const scout = { id: 0, x: 8192.5, y: 8192.5 };
// An id the client never hears of: what struck is hidden, or is an animal.
const foreign = (tick: number): Hit => [900, tick];
const own = (tick: number): Hit => [scout.id, tick];

/** Exactly those world points carry a star, now and a moment later. */
async function marked(page: Page, ...places: Point[]) {
  const check = async () => {
    const seen = [];
    for (const place of places) seen.push(await shown(page, place));
    await expectStars(page, seen, 2.5);
  };
  await expect(check).toPass({ timeout: 5_000 });
  await page.waitForTimeout(300);
  await check();
}

test("only a new foreign hit on his own unit off the screen is an alert, and Space goes there", async ({
  page,
}) => {
  test.setTimeout(60_000);
  const loud = { id: 2, x: 3000.5, y: 3000.5 };
  const friendly = { id: 3, x: 12000.5, y: 4000.5 };
  const wounded = { id: 4, x: 5000.5, y: 12000.5, hit: foreign(995) };
  const rival = { id: 5, x: 12000.5, y: 12000.5, player: 1 };
  const garrisoned = { id: 6, x: 8000.5, y: 2000.5, hit: foreign(996) };
  const released = { id: 7, x: 2000.5, y: 8000.5, hit: foreign(50) };
  // Told two seconds late: a hit that old is not looked up, let alone told.
  const late = { id: 8, x: 14000.5, y: 9000.5 };
  const game = await battle(page, home, [
    scout,
    loud,
    friendly,
    wounded,
    rival,
    garrisoned,
    late,
  ]);
  // Hits older than the page are no alert, for the key neither.
  await freshHud(page);
  await freshHud(page);
  await page.keyboard.press("Space");
  expect(await page.evaluate(() => location.hash)).toBe(`#${home}`);
  expect(await stars(page)).toEqual([]);

  const next = game.next();
  // One message, so one star says that all of it was read.
  game.send({
    units: [
      { ...scout, hit: foreign(next) },
      { ...loud, hit: foreign(next - 10) },
      { ...friendly, hit: own(next) },
      { ...wounded, x: wounded.x + 1 },
      { ...rival, hit: own(next) },
      released,
      { ...late, hit: foreign(next - 40) },
    ],
    removals: [garrisoned.id],
  });
  await marked(page, loud);
  await page.keyboard.press("Space");
  const minimap = page.locator("#minimap");
  await expect(minimap).toHaveAttribute("data-camera-x", String(loud.x));
  await expect(minimap).toHaveAttribute("data-camera-y", String(loud.y));
  await expect(minimap).toHaveAttribute("data-zoom", "1");
});

test("a building is an alert at its middle when struck and when lost in a fight, not when deleted", async ({
  page,
}) => {
  test.setTimeout(60_000);
  const struck = { id: 20, x: 14000, y: 8000 };
  const besieged = { id: 21, x: 8000, y: 14000, hit: foreign(995) };
  const deleted = { id: 22, x: 4000, y: 6000 };
  const rival = { id: 23, x: 10000, y: 6000, player: 1 };
  const damaged = { id: 24, x: 6000, y: 4000, hit: foreign(996) };
  const quiet = { id: 25, x: 9000, y: 11000, hit: foreign(100) };
  const game = await battle(
    page,
    home,
    [scout],
    [struck, besieged, deleted, rival, damaged, quiet],
  );
  const next = game.next();
  game.send({
    buildings: [
      { ...struck, hit: foreign(next) },
      { ...rival, hit: own(next) },
      damaged,
    ],
    lost: [besieged.id, deleted.id, quiet.id],
  });
  await marked(
    page,
    { x: struck.x + 2, y: struck.y + 2 },
    { x: besieged.x + 2, y: besieged.y + 2 },
  );
});

test("a wall lost unhurt is an alert at its middle and one he deleted is not", async ({
  page,
}) => {
  test.setTimeout(60_000);
  const palisade = { id: 30, x: 14000, y: 8000, kind: 7 } as const;
  const deleted = { id: 31, x: 8000, y: 14000, kind: 21 } as const;
  const game = await battle(page, home, [scout], [palisade, deleted]);
  const menu = (action: number, code: number, target = 0) =>
    page.evaluate(
      ([action, code, target]) => {
        const { command } = Reflect.get(window, "aoeBuildings") as {
          command(...order: number[]): void;
        };
        command(action, code, target, 1);
      },
      [action, code, target] as const,
    );
  // Selects the wall and shows it, so the camera has to leave it again.
  await menu(14, 0, deleted.id);
  await page.evaluate(() => {
    location.hash = "x=8300.5&y=8192.5&zoom=1";
  });
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-camera-x",
    "8300.5",
  );
  await game.settled();
  await menu(3, 4);
  await expect.poll(game.orders).toContain(31);
  game.send({ lost: [palisade.id, deleted.id] });
  await marked(page, { x: palisade.x + 0.5, y: palisade.y + 0.5 });
});

test("a hit by a boar the client knows is no alert, by a wolf or a stranger it is", async ({
  page,
}) => {
  test.setTimeout(60_000);
  const hunter = { id: 2, x: 3000.5, y: 3000.5 };
  const prey = { id: 3, x: 12000.5, y: 4000.5 };
  const victim = { id: 4, x: 5000.5, y: 12000.5 };
  const game = await battle(
    page,
    home,
    [scout, hunter, prey, victim],
    [],
    [
      { id: 40, x: 3001.5, y: 3000.5, kind: 2 },
      { id: 41, x: 12001.5, y: 4000.5, kind: 3 },
    ],
  );
  const next = game.next();
  game.send({
    units: [
      { ...hunter, hit: [40, next] },
      { ...prey, hit: [41, next] },
      { ...victim, hit: foreign(next) },
    ],
  });
  await marked(page, prey, victim);
});

test("at marker zoom a hit in the middle of the screen is an alert too", async ({
  page,
}) => {
  test.setTimeout(60_000);
  // No camera pull-back of the first far view.
  await page.emulateMedia({ reducedMotion: "reduce" });
  const game = await battle(page, "x=8192.5&y=8192.5&zoom=.05", [scout]);
  const html = page.locator("html");
  await expect(html).toHaveAttribute("data-lod", /^(map|markers)$/);
  await page.waitForFunction(
    () => sessionStorage.getItem("aoeworld.cockpit-reveal") !== null,
  );
  await expect(html).not.toHaveAttribute("data-cockpit-reveal", /.+/, {
    timeout: 15_000,
  });
  await game.settled();
  game.send({ units: [{ ...scout, hit: foreign(game.next()) }] });
  await marked(page, scout);
});

for (const action of ["pan", "reconnection"] as const)
  test(`a hit first told by the snapshot of a ${action} is no alert`, async ({
    page,
  }) => {
    test.setTimeout(60_000);
    const hidden = { id: 2, x: 3000.5, y: 3000.5 };
    const loud = { id: 3, x: 12000.5, y: 4000.5 };
    const game = await battle(page, home, [scout, hidden, loud]);
    const struck = { ...hidden, hit: foreign(game.now()) };
    game.hold(struck);
    const before = game.views();
    await page.evaluate((action) => {
      if (action === "pan") location.hash = "x=8300.5&y=8192.5&zoom=1";
      else document.getElementById("reconnect")?.click();
    }, action);
    await expect.poll(game.views).toBeGreaterThan(before);
    await expect(page.locator("#connection")).toHaveText("connected");
    await game.settled();
    game.send({
      units: [
        { ...struck, x: struck.x + 1 },
        { ...loud, hit: foreign(game.next()) },
      ],
    });
    await marked(page, loud);
  });
