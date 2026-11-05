import { expect, type Page } from "@playwright/test";
import { test } from "../../../server.js";
import { readFile } from "node:fs/promises";

declare global {
  interface Window {
    aoeSteward: {
      update(values: number[], label: string, settings: number[]): void;
      settings(): Uint8Array;
      bind(callback: () => void): void;
    };
    confirmedSteward: () => number[];
  }
}

test.use({ video: "on" });

async function panel(page: Page) {
  await page.setContent('<main id="selection-content"></main>');
  await page.addStyleTag({
    content: await readFile("../web/interface/game.css", "utf8"),
  });
  const source = await readFile("../web/interface/steward/panel.js", "utf8");
  await page.addScriptTag({
    // The panel is a module; this page has no origin to import its slider fill from.
    content:
      source
        .replace(/^import .*$/m, "const fillRange = () => {};")
        .replace("export function", "function") + "\ninitSteward();",
  });
  await page.evaluate(() => {
    const api = window.aoeSteward;
    let confirmed = [17, 45, 30, 15, 10, 100, 0];
    window.confirmedSteward = () => confirmed;
    api.update([], "Steward settings", confirmed);
    api.bind(() => {
      const submitted = Array.from(api.settings()) as number[];
      if (submitted.slice(0, 4).some((v) => v > 0))
        confirmed = [17, ...submitted];
      // A rejected order sends no changed settings: ordinary HUD refreshes
      // repeat the same authoritative signature.
      api.update([], "Steward settings", confirmed);
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

test("training portraits retain wire slots and append the Steward's real King icon", async ({
  page,
}) => {
  await page.goto("/");
  const art = await page.evaluate(async () => {
    const source = "/interface/ui-art.js";
    const { trainingKinds, entityIcon } = await import(source);
    return { names: trainingKinds, steward: entityIcon(trainingKinds[34]) };
  });
  expect(art.names).toHaveLength(35);
  expect(art.names[22]).toBe("Vassal");
  expect(art.names[23]).toBe("General");
  expect(art.names[33]).toBe("Onager");
  expect(art.names[34]).toBe("Steward");
  expect(art.steward).toContain('data-ui-id="434"');
});
