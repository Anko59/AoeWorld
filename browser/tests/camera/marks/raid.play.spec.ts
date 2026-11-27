import { expect, type Page } from "@playwright/test";
import { test } from "../../../server.js";
import { pixels, stillMap, type Still } from "../still-map.js";

const ours = { owner: 1, colour: 0x0000ff };
const theirs = { owner: 2, colour: 0xff0000 };

type Town = {
  /** In a square, that many px apart; `towns`: the keeps it shows. */
  buildings: number;
  apart: number;
  towns: number;
  /** Own villagers on every cell of the town and around it. */
  villagers?: boolean;
  /** A rival building where the raider stands. */
  tower?: boolean;
  raiders?: number;
};

/** Draws an own town round the middle of the map, and its raiders there. */
const draw = (page: Page, town: Town) =>
  page.evaluate(
    async ({ town, ours, theirs }) => {
      const still = Reflect.get(window, "still") as Still;
      const { buildings, apart, villagers, tower, raiders = 1 } = town;
      const side = Math.ceil(Math.sqrt(buildings));
      const middle = ((side - 1) * apart) / 2;
      const rival = (kind: number) => [
        kind,
        ...still.at(0.2, 0.2),
        theirs.owner,
        theirs.colour,
      ];
      still.present(...still.at(0, -55));
      still.actors([
        ...Array.from({ length: buildings }, (_, at) => [
          9,
          ...still.at(
            (at % side) * apart - middle,
            Math.floor(at / side) * apart - middle,
          ),
          ours.owner,
          ours.colour,
        ]).flat(),
        ...(villagers
          ? still.block([0, 0], [20, 20], { kind: 8, ...ours })
          : []),
        ...(tower ? rival(1) : []),
        ...Array.from({ length: raiders }, () => rival(0)).flat(),
      ]);
      await still.frames(2);
      return document.querySelectorAll("#minimap-marks [data-town]").length;
    },
    { town, ours, theirs },
  );

/** The colours of the page within that many px of a place of the map. */
async function around(page: Page, right: number, down: number, reach: number) {
  const map = await page.evaluate(
    () => (Reflect.get(window, "still") as Still).map,
  );
  const colour = await pixels(page);
  const steps = Array.from({ length: 2 * reach + 1 }, (_, at) => at - reach);
  return steps.flatMap((across) =>
    steps.map((below) =>
      colour(
        map.left + map.width / 2 + right + across,
        map.top + map.height / 2 + down + below,
      ),
    ),
  );
}

for (const frame of ["native", "square", "overview"] as const)
  test(`one raider in an own town shows on the ${frame} map`, async ({
    page,
  }) => {
    test.setTimeout(60_000);
    await stillMap(page, frame);
    // A unit's red; that of a building is darker.
    const reds = async () =>
      (await around(page, 0, 0, 12)).filter(
        ({ red, green, blue }) => red > 190 && Math.max(green, blue) < 90,
      ).length;
    const towns: [string, Town][] = [
      ["a keep", { buildings: 9, apart: 2, towns: 1 }],
      ["its villagers", { buildings: 9, apart: 2, towns: 1, villagers: true }],
      ["a keep over blocks", { buildings: 25, apart: 4, towns: 1 }],
      ["single buildings", { buildings: 5, apart: 3, towns: 0 }],
      [
        "single buildings and villagers",
        { buildings: 5, apart: 3, towns: 0, villagers: true },
      ],
      [
        "villagers round the rival's tower",
        { buildings: 5, apart: 3, towns: 0, villagers: true, tower: true },
      ],
    ];
    for (const [name, town] of towns) {
      expect(await draw(page, town), name).toBe(town.towns);
      expect(await reds(), name).toBeGreaterThanOrEqual(4);
      // Without its raider the same town has no such px.
      await draw(page, { ...town, raiders: 0 });
      expect(await reds(), `${name}, nobody there`).toBe(0);
    }
  });

test("an own scout shows in a rival town, and villagers leave their keep plain", async ({
  page,
}) => {
  await stillMap(page, "native");
  // The own units are green, to tell them from their blue buildings.
  const unit = { owner: ours.owner, colour: 0x00ff00 };
  const towns = await page.evaluate(
    async ({ ours, theirs, unit }) => {
      const still = Reflect.get(window, "still") as Still;
      const town = (
        right: number,
        kind: number,
        { owner, colour }: typeof ours,
      ) =>
        Array.from({ length: 9 }, (_, at) => [
          kind,
          ...still.at(right - 2 + (at % 3) * 2, Math.floor(at / 3) * 2 - 2),
          owner,
          colour,
        ]).flat();
      still.present(...still.at(0, -80));
      still.actors([
        ...town(-50, 9, ours),
        ...still.block([-50, 0], [8, 8], { kind: 8, ...unit }),
        ...town(50, 1, theirs),
        ...[8, ...still.at(50, 0), unit.owner, unit.colour],
      ]);
      await still.frames(2);
      return document.querySelectorAll("#minimap-marks [data-town]").length;
    },
    { ours, theirs, unit },
  );
  expect(towns).toBe(2);
  const keep = await around(page, -50, 0, 2);
  expect(
    keep.filter(({ green, blue }) => green > 90 && green > blue + 30),
    "villagers over their keep",
  ).toEqual([]);
  const scout = (await around(page, 50, 0, 3)).filter(
    ({ red, green, blue }) => green > 190 && Math.max(red, blue) < 90,
  );
  expect(scout.length, "the own scout").toBeGreaterThanOrEqual(4);
});

for (const frame of ["native", "square", "overview"] as const)
  test(`a ping over a keep is as plain as over open land on the ${frame} map`, async ({
    page,
  }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await stillMap(page, frame);
    const places = [-20, 20] as const;
    await page.evaluate(async ({ owner, colour }) => {
      const still = Reflect.get(window, "still") as Still;
      still.present(...still.at(0, -55));
      still.actors(
        Array.from({ length: 9 }, (_, at) => [
          9,
          ...still.at(-22 + (at % 3) * 2, Math.floor(at / 3) * 2 - 2),
          owner,
          colour,
        ]).flat(),
      );
      await still.frames(2);
    }, ours);
    const looks = async () =>
      [
        await around(page, places[0], 0, 10),
        await around(page, places[1], 0, 10),
      ] as const;
    const bare = await looks();
    await page.evaluate(async (places) => {
      const navigation = Reflect.get(window, "aoeNavigation") as {
        bindOrders(send: () => number): void;
      };
      const still = Reflect.get(window, "still") as Still;
      // A timer ends a ping: none may while the screenshots are taken.
      Reflect.set(window, "setTimeout", () => 0);
      navigation.bindOrders(() => 2);
      for (const right of places)
        document.getElementById("minimap-map")?.dispatchEvent(
          new PointerEvent("pointerdown", {
            bubbles: true,
            cancelable: true,
            button: 2,
            clientX: still.map.left + still.map.width / 2 + right,
            clientY: still.map.top + still.map.height / 2,
          }),
        );
      await still.frames(2);
    }, places);
    const pinged = await looks();
    const changed = (place: 0 | 1) =>
      bare[place].filter((was, at) => {
        const is = pinged[place][at] ?? was;
        return (
          Math.max(
            Math.abs(was.red - is.red),
            Math.abs(was.green - is.green),
            Math.abs(was.blue - is.blue),
          ) > 8
        );
      }).length;
    expect(changed(1), "over open land").toBeGreaterThan(60);
    expect(changed(0), "over the keep").toBeGreaterThan(changed(1) * 0.6);
  });

test("a ping keeps its place while the minimap zooms and follows the camera", async ({
  page,
}) => {
  await stillMap(page, "native");
  const apart = await page.evaluate(async () => {
    const navigation = Reflect.get(window, "aoeNavigation") as {
      bindOrders(send: () => number): void;
    };
    const still = Reflect.get(window, "still") as Still;
    const { minimapProject } = await import("/minimap.js" as string);
    const svg = document.getElementById("minimap-map");
    if (!svg) throw new Error("#minimap-map is missing");
    Reflect.set(window, "setTimeout", () => 0);
    navigation.bindOrders(() => 2);
    still.present(1000, 1000);
    const { left, top, width, height } = still.map;
    const point = still.at(30, 12);
    const pointer = (type: string, button: number, x: number, y: number) =>
      svg.dispatchEvent(
        new PointerEvent(type, {
          bubbles: true,
          cancelable: true,
          button,
          clientX: left + width / 2 + x,
          clientY: top + height / 2 + y,
        }),
      );
    pointer("pointerdown", 2, 30, 12);
    const apart: number[] = [];
    for (const [factor, camera] of [
      [1, 1000],
      [3, 1000],
      [1, 1060],
    ] as const) {
      still.wheel(factor);
      still.present(camera, camera);
      await still.frames(1);
      const box = document
        .querySelector("[data-minimap-ping]")
        ?.getBoundingClientRect();
      const zoom = Number(
        document.getElementById("minimap")?.dataset["minimapZoom"],
      );
      const place: number[] = minimapProject(point, [2000, 2000], {
        zoom,
        center: zoom === 1 ? [1000, 1000] : [camera, camera],
      });
      apart.push(
        Math.hypot(
          (box?.x ?? NaN) +
            (box?.width ?? NaN) / 2 -
            left -
            ((place[0] ?? NaN) * width) / 256,
          (box?.y ?? NaN) +
            (box?.height ?? NaN) / 2 -
            top -
            ((place[1] ?? NaN) * height) / 256,
        ),
      );
    }
    return apart;
  });
  expect(apart).toHaveLength(3);
  for (const distance of apart) expect(distance).toBeLessThan(2);
});
