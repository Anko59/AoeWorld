import { expect } from "@playwright/test";
import { test } from "../../../server.js";

test("General roster and trainer state follow the selected commander", async ({
  page,
}) => {
  await page.goto("/?skipIntro=1");
  await page.setContent(
    '<!doctype html><link rel="stylesheet" href="/interface/game.css"><body></body>',
  );
  await page.evaluate(async () => {
    if (!document.getElementById("general-panel")) {
      const path = "/interface/general-panel.js";
      const panel = (await import(path)) as { initGeneralPanel: () => void };
      panel.initGeneralPanel();
    }
    const w = window as unknown as {
      aoeBuildings: { command: (a: number, c: number) => void };
      squadCalls: number[][];
    };
    w.squadCalls = [];
    w.aoeBuildings = { command: (a, c) => w.squadCalls.push([a, c]) };
  });
  const infantry = [
    11, 20, 1, 1, 2, 35, 30, 20, 10, 5, 20, 0, 0, 0, 0, 1, 101, 0, 20, 1,
  ];
  const siege = [
    22, 5, 0, 0, 0, 0, 0, 0, 100, 0, 0, 0, 0, 5, 0, 1, 201, 3, 5, 0,
  ];
  const update = (values: number[]) =>
    page.evaluate((v) => {
      (
        window as unknown as { aoeGeneral: { update: (v: number[]) => void } }
      ).aoeGeneral.update(v);
    }, values);
  const panel = page.locator("#general-panel");
  await update(infantry);
  await expect(panel.locator("#general-squads li")).toHaveCount(1);
  await expect(panel.locator("#general-squads")).toContainText("Infantry");
  await expect(panel.locator("#general-squads")).toContainText("20/20");
  await expect(panel.locator("#general-squads")).toContainText("marching");
  await expect(panel.locator('[data-ui-id="432"]')).toHaveAttribute(
    "data-native-only",
    "true",
  );
  await page.locator("#general-trainer").click();
  await page.getByRole("slider", { name: "Infantry share" }).focus();
  await update(siege);
  await expect(panel).toHaveAttribute("data-general", "22");
  await expect(panel).toHaveAttribute("data-mode", "captain");
  await expect(panel.locator("#general-squads")).toContainText("Siege");
  await expect(panel.locator("#general-squads")).toContainText("5/5");
  await page.locator("#general-trainer").click();
  await expect(
    page.getByRole("slider", { name: "Infantry share" }),
  ).toHaveValue("0");
  await expect(page.getByRole("slider", { name: "Siege share" })).toHaveValue(
    "100",
  );
  await page.locator("#general-train").click();
  await update(infantry);
  await expect(panel).toHaveAttribute("data-general", "11");
  await expect(panel).toHaveAttribute("data-mode", "trainer");
  await expect(
    page.getByRole("slider", { name: "Infantry share" }),
  ).toHaveValue("35");
  await page.locator("#general-train").click();
  expect(
    await page.evaluate(
      () => (window as unknown as { squadCalls: number[][] }).squadCalls,
    ),
  ).toEqual([
    [9, 1],
    [9, 0],
  ]);
  await update([]);
  await expect(panel).toBeHidden();
});
