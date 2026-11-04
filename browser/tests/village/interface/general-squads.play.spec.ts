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

test("General controls and the kingdom cockpit remain usable together at far zoom", async ({
  page,
}) => {
  // Serve a component fixture at the app origin without starting the game client.
  // A live client could otherwise overwrite the fixture's connection status.
  await page.route("**/__general-panel-fixture", (route) =>
    route.fulfill({
      contentType: "text/html",
      body: '<!doctype html><link rel="stylesheet" href="/interface/game.css"><body><canvas id="scene"></canvas><span id="connection">connected</span></body>',
    }),
  );
  await page.goto("/__general-panel-fixture");
  await page.evaluate(async () => {
    const generalPath = "/interface/general-panel.js";
    const cockpitPath = "/navigation/cockpit.js";
    const { initGeneralPanel } = await import(generalPath);
    const { initCockpit } = await import(cockpitPath);
    const w = window as unknown as {
      aoeBuildings: { command: (action: number, code: number) => void };
      aoeGeneral: { update: (values: number[]) => void };
      panelCalls: number[][];
    };
    w.panelCalls = [];
    w.aoeBuildings = {
      command: (action, code) => w.panelCalls.push([action, code]),
    };
    initGeneralPanel();
    // Match runtime append order: the cockpit is created after the General panel.
    const cockpit = initCockpit();
    cockpit.update(
      [1000, 1000, 1000, 1000, 20, 100, 0, 0, 0, 0, 40],
      Array(35).fill(1),
    );
    cockpit.building.ready(Array(21).fill(1));
    cockpit.show(true);
    w.aoeGeneral.update([
      11, 20, 0, 0, 0, 35, 30, 20, 10, 5, 20, 0, 0, 0, 0, 1, 101, 0, 20, 0,
    ]);
  });
  for (const lod of ["markers", "map"]) {
    await page.evaluate((value) => {
      document.documentElement.dataset.lod = value;
    }, lod);
    await page.locator("#general-trainer").click({ timeout: 2000 });
    await page.getByRole("slider", { name: "Infantry share" }).focus();
    await page.keyboard.press("Home");
    await page.locator("#general-train").click();
    await page.locator("#general-captain").click();
    await page.getByLabel("Building type").selectOption("1");
    await page.getByRole("button", { name: "Build here", exact: true }).click();
    await page.getByRole("button", { name: "Units here", exact: true }).click();
    await page.keyboard.press("Escape");
    await expect(page.locator("#strategic-cockpit")).toBeVisible();
    await expect(page.locator("#general-squads")).toContainText("20/20");
  }
  const calls = await page.evaluate(
    () => (window as unknown as { panelCalls: number[][] }).panelCalls,
  );
  expect(calls.filter(([action]) => action === 9)).toEqual([
    [9, 1],
    [9, 1],
  ]);
  expect(calls.filter(([action]) => action === 10)).toEqual([[10, 0]]);
  expect(calls.filter(([action]) => action === 0)).toHaveLength(2);
  expect(calls.filter(([action]) => action === 11)).toHaveLength(2);
});
