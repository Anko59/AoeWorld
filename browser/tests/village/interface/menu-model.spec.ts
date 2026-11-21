import { expect } from "@playwright/test";
import { test } from "../../../server.js";

test("the menu describes every card shape and limits commands to owned selections", async ({
  page,
}) => {
  await page.goto("/");
  const results = await page.evaluate(async () => {
    const source = "/interface/menu/selection.js";
    const { describeSelection } = await import(source);
    const unit = { kind: "Villager", id: 1, player: 0, owned: true, hp: 25 };
    const fighter = { ...unit, kind: "LightCavalry", id: 2, attack: 7 };
    const house = { ...unit, kind: "House", id: 3 };
    const enemy = { ...fighter, player: 1, owned: false };
    const sheep = { ...unit, kind: "Sheep", player: -1, owned: false };
    const rules = {
      Villager: { moves: true },
      LightCavalry: { moves: true, stance: true, formation: true },
      General: {},
      FishingShip: {},
      TransportShip: {},
      TradeCog: {},
      Monk: {},
    };
    return [
      describeSelection([], null),
      describeSelection([{ ...unit, attack: 3 }], null, rules),
      describeSelection([fighter], null, rules),
      describeSelection([house], null, rules),
      describeSelection([fighter, { ...fighter, id: 4 }], null, rules),
      describeSelection([house, { ...house, id: 4 }], null, rules),
      describeSelection([fighter, unit], null, rules),
      describeSelection([house, fighter], null, rules),
      describeSelection([], { kind: 0, amount: 25 }),
      describeSelection([sheep], null),
      describeSelection([enemy], null),
      describeSelection([{ ...house, player: 1, owned: false }], null),
      ...["General", "FishingShip", "TransportShip", "TradeCog", "Monk"].map(
        (kind) => describeSelection([{ ...unit, kind }], null, rules),
      ),
    ].map(
      ({
        type,
        name,
        commands,
      }: {
        type: string;
        name: string;
        commands: string[];
      }) => ({ type, name, commands }),
    );
  });
  expect(results.map(({ type }) => type)).toEqual([
    "empty",
    "unit",
    "unit",
    "building",
    "units",
    "buildings",
    "units",
    "mixed",
    "resource",
    "animal",
    "enemy",
    "enemy",
    "unit",
    "unit",
    "unit",
    "unit",
    "unit",
  ]);
  expect(results[1]?.commands).toContain("attack-move-units");
  expect(results[1]?.commands).not.toContain("patrol-units");
  expect(results[4]?.name).toBe("Light Cavalry x2");
  expect(results[6]?.commands).toContain("attack-move-units");
  expect(results[6]?.commands).toContain("patrol-units");
  expect(results[7]?.commands).toContain("stop-units");
  for (const index of [0, 8, 9, 10, 11])
    expect(results[index]?.commands).toEqual([]);
  expect(results[12]?.commands).toContain("general-only-move");
  expect(results.slice(13).map(({ name }) => name)).toEqual([
    "Fishing Ship",
    "Transport Ship",
    "Trade Cog",
    "Monk",
  ]);
  for (const entry of results.slice(13))
    expect(entry.commands).toEqual([
      "move-units",
      "stop-units",
      "delete-units",
    ]);
});

test("command pages keep all commands reachable with unique displayed shortcuts", async ({
  page,
}) => {
  await page.goto("/");
  const pages = await page.evaluate(async () => {
    const source = "/interface/menu/render.js";
    const { commandSlots } = await import(source);
    const commands = Array.from({ length: 27 }, (_, index) => ({
      id: `order-${index}`,
      key: index < 2 ? "Q" : "",
    }));
    return [0, 1, 2].map(
      (page) =>
        commandSlots(commands, page).slots.filter(Boolean) as {
          id: string;
          key: string;
        }[],
    );
  });
  for (const page of pages) {
    const keys = page.map((command) => command.key);
    expect(keys.every(Boolean)).toBe(true);
    expect(new Set(keys).size).toBe(keys.length);
  }
  expect(pages[0]?.[0]).toMatchObject({ id: "order-0", key: "Q" });
  expect(
    pages
      .flatMap((page) =>
        page.filter(({ id }) => id.startsWith("order-")).map(({ id }) => id),
      )
      .sort(),
  ).toEqual(Array.from({ length: 27 }, (_, index) => `order-${index}`).sort());
});

test("Watch Tower is the only tower foundation in the build grid", async ({
  page,
}) => {
  await page.goto("/");
  const towers = await page.evaluate(async () => {
    const catalogSource = "/interface/menu/catalog.js";
    const commandsSource = "/interface/menu/commands.js";
    const { buildableKinds, buildingKinds } = await import(catalogSource);
    const { describeCommands } = await import(commandsSource);
    const rules = {
      buildings: buildingKinds.map(() => ({
        age: 0,
        buildable: true,
        cost: [],
      })),
    };
    return ["economy", "military"].flatMap((page) =>
      describeCommands(
        { villagers: true, commands: [] },
        { age: 0, build: buildableKinds.map(() => 1) },
        {},
        rules,
        { page },
      ).filter(({ id }: { id: string }) =>
        ["build-watch-tower", "build-guard-tower", "build-keep"].includes(id),
      ),
    );
  });
  expect(towers).toEqual([
    expect.objectContaining({
      id: "build-watch-tower",
      order: [0, 13],
      disabled: false,
    }),
  ]);
});
