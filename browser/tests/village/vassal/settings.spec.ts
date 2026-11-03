import { expect, type Page } from "@playwright/test";
import { test } from "../../../server.js";
import { readFile } from "node:fs/promises";

declare global {
  interface Window {
    aoeVassal: {
      update(values: number[], label: string, settings: number[]): void;
      settings(): Uint8Array;
      bind(callback: () => void): void;
    };
    confirmedVassal: () => number[];
  }
}

test.use({ video: "on" });

async function panel(page: Page) {
  await page.setContent('<main id="selection-content"></main>');
  await page.addStyleTag({
    content: await readFile("../web/interface/game.css", "utf8"),
  });
  const source = await readFile("../web/interface/vassal/panel.js", "utf8");
  await page.addScriptTag({
    content: source.replace("export function", "function") + "\ninitVassal();",
  });
  await page.evaluate(() => {
    const api = window.aoeVassal;
    let confirmed = [17, 45, 30, 15, 10, 100, 0];
    window.confirmedVassal = () => confirmed;
    api.update([], "Vassal settings", confirmed);
    api.bind(() => {
      const submitted = Array.from(api.settings()) as number[];
      if (submitted.slice(0, 4).some((v) => v > 0))
        confirmed = [17, ...submitted];
      // A rejected order sends no changed settings: ordinary HUD refreshes
      // repeat the same authoritative signature.
      api.update([], "Vassal settings", confirmed);
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
  expect(await page.evaluate(() => window.confirmedVassal())).toEqual([
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
  await expect(page.locator("#vassal-setting-3 + output")).toHaveText("10");
});
