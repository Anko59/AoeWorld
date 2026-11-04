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

test("attacked Vassal shows its alert while a nearby General arrives and returns", async ({
  page,
  request,
}, testInfo) => {
  test.setTimeout(240_000);
  const { gameAssets } = await import("../../game-assets.js");
  await gameAssets(page, true);
  expect(
    (await request.post("/maps/reset?scenario=vassal-defense")).status(),
  ).toBe(204);
  await page.goto("/?play=1#x=108&y=94&zoom=0.25");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  const samples: unknown[] = [];
  const timer = setInterval(() => {
    void page
      .evaluate(() => ({
        clock: document.getElementById("game-clock")?.textContent,
        status: document.getElementById("vassal-status")?.textContent,
        actors: (
          JSON.parse(
            document.getElementById("scene")?.getAttribute("data-actors") ??
              "[]",
          ) as {
            kind: string;
            player: number;
            x: number;
            y: number;
            task: string;
          }[]
        )
          .filter(
            (a) =>
              a.kind === "General" || a.kind === "Castle" || a.player === 1,
          )
          .map(({ kind, player, x, y, task }) => ({
            kind,
            player,
            x,
            y,
            task,
          })),
      }))
      .then((sample) => samples.push(sample))
      .catch(() => undefined);
  }, 1_000);
  try {
    await expect(async () => {
      const actors = JSON.parse(
        (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
      ) as { kind: string; x: number; y: number }[];
      const castle = actors.find((a) => a.kind === "Castle");
      if (!castle) throw new Error("Castle has not rendered");
      await page.mouse.click(castle.x, castle.y + 2);
      await expect(page.locator("#vassal-panel")).toBeVisible({
        timeout: 1_000,
      });
    }).toPass({ timeout: 20_000 });
    const generalPosition = async () => {
      const actors = JSON.parse(
        (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
      ) as { kind: string; x: number; y: number }[];
      const general = actors.find((a) => a.kind === "General");
      if (!general) throw new Error("General has not rendered");
      return general;
    };
    const home = await generalPosition();
    await expect(page.locator("#vassal-status")).toContainText("UNDER ATTACK", {
      timeout: 180_000,
    });
    await expect(page.locator("#vassal-status")).toContainText(
      "1 General(s) responding",
    );
    await expect(async () => {
      expect((await generalPosition()).x).toBeLessThan(home.x - 200);
    }).toPass({ timeout: 20_000 });
    await expect(page.locator("#vassal-status")).toContainText(
      "holding territory",
      { timeout: 60_000 },
    );
    await expect(async () => {
      const general = await generalPosition();
      expect(Math.abs(general.x - home.x)).toBeLessThan(4);
      expect(Math.abs(general.y - home.y)).toBeLessThan(4);
    }).toPass({ timeout: 30_000 });
  } finally {
    clearInterval(timer);
    await testInfo.attach("vassal-match-observations", {
      body: JSON.stringify(samples),
      contentType: "application/json",
    });
  }
});
