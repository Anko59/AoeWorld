import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { readFile } from "node:fs/promises";

test("Vassal panel explains completion, alerts and sends pause/resume", async ({
  page,
}) => {
  await page.setContent('<main id="selection-content"></main>');
  const source = await readFile("../web/interface/steward/vassal.js", "utf8");
  await page.addScriptTag({
    content: source.replace("export function", "function") + "\ninitVassal();",
  });
  await page.evaluate(() => {
    const w = window as unknown as {
      aoeVassal: {
        update(v: number[], label: string): void;
        bind(cb: () => void): void;
      };
      sent: number;
    };
    w.sent = 0;
    w.aoeVassal.bind(() => {
      w.sent += 1;
    });
    w.aoeVassal.update(
      [12, 1],
      "Vassal · 20/20 villagers · 20/20 military · building towers · developed at 77/77 completed sites",
    );
  });
  await expect(page.locator("#vassal-status")).toContainText("20/20 military");
  await expect(page.locator("#vassal-status")).toContainText(
    "developed at 77/77",
  );
  await page
    .getByRole("button", { name: "Stop development", exact: true })
    .click();
  expect(
    await page.evaluate(() => (window as unknown as { sent: number }).sent),
  ).toBe(1);
  await page.evaluate(() => {
    (
      window as unknown as {
        aoeVassal: { update(v: number[], label: string): void };
      }
    ).aoeVassal.update(
      [12, 0],
      "UNDER ATTACK · 1 General responding · development paused",
    );
  });
  await expect(page.locator("#vassal-status")).toContainText("UNDER ATTACK");
  await page
    .getByRole("button", { name: "Resume development", exact: true })
    .click();
  expect(
    await page.evaluate(() => (window as unknown as { sent: number }).sent),
  ).toBe(2);
});

test("Castle Vassal report and development toggle use the authoritative server", async ({
  page,
  request,
}) => {
  const { gameAssets } = await import("../../game-assets.js");
  await gameAssets(page, true);
  expect(
    (await request.post("/maps/reset?scenario=vassal-defense")).status(),
  ).toBe(204);
  await page.goto("/?play=1#x=92&y=92&zoom=1");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await expect(async () => {
    const actors = JSON.parse(
      (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
    ) as { kind: string; x: number; y: number }[];
    const castle = actors.find((a) => a.kind === "Castle");
    if (!castle) throw new Error("Castle has not rendered yet");
    await page.mouse.click(castle.x, castle.y - 8);
    await expect(page.locator("#vassal-status")).toContainText(
      "/20 villagers",
      { timeout: 1_000 },
    );
  }).toPass({ timeout: 20_000 });
  await page
    .getByRole("button", { name: "Stop development", exact: true })
    .click();
  await expect(page.locator("#vassal-status")).toContainText(
    "development paused",
  );
  await expect(page.locator("#vassal-status")).toContainText("0/20 villagers");
  await page
    .getByRole("button", { name: "Resume development", exact: true })
    .click();
  await expect(page.locator("#vassal-status")).toContainText("20/20 villagers");
});
