import { expect, type Page } from "@playwright/test";
import { test } from "../../../server.js";

declare global {
  interface Window {
    aoeSteward: {
      update(values: number[], label: string, settings: number[]): void;
      settings(): Uint8Array;
    };
    confirmedSteward: () => number[];
  }
}

test.use({ video: "on" });

async function panel(page: Page) {
  await page.route("**/steward-settings-fixture", (route) =>
    route.fulfill({
      contentType: "text/html",
      body: '<main id="selection-content"></main>',
    }),
  );
  await page.goto("/steward-settings-fixture");
  await page.evaluate(async () => {
    const source = "/interface/steward/panel.js";
    const { initSteward } = await import(source);
    initSteward();
    const api = window.aoeSteward;
    let confirmed = [17, 45, 30, 15, 10, 100, 0];
    window.confirmedSteward = () => confirmed;
    api.update([], "Steward settings", confirmed);
    Reflect.set(window, "aoeBuildings", {
      command(action: number, code: number) {
        if (action !== 18 || code !== 0) throw new Error("Unexpected order");
        const submitted = Array.from(api.settings());
        if (submitted.slice(0, 4).some((v) => v > 0))
          confirmed = [17, ...submitted];
        // A rejected order sends no changed settings: ordinary HUD refreshes
        // repeat the same authoritative signature.
        api.update([], "Steward settings", confirmed);
      },
    });
  });
  for (const name of [
    "Food weight",
    "Wood weight",
    "Gold weight",
    "Stone weight",
  ])
    await page.getByLabel(name, { exact: true }).press("Home");
}

test("F2: an invalid resource mix cannot poison later cap and training changes", async ({
  page,
}) => {
  await panel(page);
  await page.getByLabel("Villager cap", { exact: true }).press("ArrowLeft");
  await page
    .getByLabel("Train villagers and fishing ships", { exact: true })
    .check();
  expect(await page.evaluate(() => window.confirmedSteward())).toEqual([
    17, 0, 0, 0, 10, 99, 1,
  ]);
  await expect(page.getByLabel("Villager cap", { exact: true })).toHaveValue(
    "99",
  );
  await expect(
    page.getByLabel("Train villagers and fishing ships", { exact: true }),
  ).toBeChecked();
});

test("F4: the last positive weight remains visible after attempting an all-zero mix", async ({
  page,
}) => {
  await panel(page);
  await expect(page.getByLabel("Stone weight", { exact: true })).toHaveValue(
    "10",
  );
  await expect(page.locator("#steward-setting-3 + output")).toHaveText("10");
});

test("a late echo of an earlier order does not undo a newer edit", async ({
  page,
}) => {
  await page.route("**/steward-settings-fixture", (route) =>
    route.fulfill({
      contentType: "text/html",
      body: '<main id="selection-content"></main>',
    }),
  );
  await page.goto("/steward-settings-fixture");
  // Orders reach the server and come back a frame or more later, here only
  // when the test delivers them.
  await page.evaluate(async () => {
    const source = "/interface/steward/panel.js";
    const { initSteward } = await import(source);
    initSteward();
    const api = window.aoeSteward;
    api.update([], "Steward settings", [17, 45, 30, 15, 10, 100, 0]);
    const sent: number[][] = [];
    Reflect.set(window, "sentSteward", sent);
    Reflect.set(window, "aoeBuildings", {
      command(action: number, code: number) {
        if (action !== 18 || code !== 0) throw new Error("Unexpected order");
        sent.push(Array.from(api.settings()));
      },
    });
  });
  const echo = (index: number) =>
    page.evaluate((index) => {
      const order = (Reflect.get(window, "sentSteward") as number[][])[index];
      if (!order) throw new Error(`Order ${index} was never sent`);
      window.aoeSteward.update([], "Steward settings", [17, ...order]);
    }, index);
  const cap = page.getByLabel("Villager cap", { exact: true });
  const training = page.getByLabel("Train villagers and fishing ships", {
    exact: true,
  });
  await page.getByLabel("Wood weight", { exact: true }).press("End");
  await cap.press("ArrowLeft");
  await echo(0);
  await expect(cap).toHaveValue("99");
  await training.check();
  await echo(1);
  await expect(training).toBeChecked();
  await echo(2);
  expect(await page.evaluate(() => Reflect.get(window, "sentSteward"))).toEqual(
    [
      [45, 100, 15, 10, 100, 0],
      [45, 100, 15, 10, 99, 0],
      [45, 100, 15, 10, 99, 1],
    ],
  );
  await expect(cap).toHaveValue("99");
  await expect(training).toBeChecked();
});

test("training portraits retain wire slots and append the Steward's real King icon", async ({
  page,
}) => {
  await page.goto("/");
  const art = await page.evaluate(async () => {
    const source = "/interface/ui-art.js";
    const { trainingKinds, entityIcon } = await import(source);
    return { names: trainingKinds, steward: entityIcon(trainingKinds[34]) };
  });
  expect(art.names).toHaveLength(60);
  expect(art.names[37]).toBe("FireGalley");
  expect(art.names[48]).toBe("CavalryArcher");
  expect(art.names[22]).toBe("Vassal");
  expect(art.names[23]).toBe("General");
  expect(art.names[33]).toBe("Onager");
  expect(art.names[34]).toBe("Steward");
  expect(art.steward).toContain('data-ui-id="434"');
});
