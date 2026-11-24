import { expect } from "@playwright/test";
import { test } from "../../../server.js";

test.use({ video: { mode: "on", size: { width: 1280, height: 720 } } });

test("Vassal panel shows work, switches Vassals and sends pause/resume", async ({
  page,
}) => {
  await page.route("**/vassal-panel-fixture", (route) =>
    route.fulfill({
      contentType: "text/html",
      body: '<main id="selection-content"></main>',
    }),
  );
  await page.goto("/vassal-panel-fixture");
  await page.evaluate(async () => {
    const module = "/interface/steward/panel.js";
    const { initVassal } = await import(module);
    initVassal();
    const w = window as unknown as {
      aoeVassal: {
        update(v: number[], label: string, jobs: string, squads: string): void;
      };
      sent: number;
    };
    w.sent = 0;
    Reflect.set(window, "aoeBuildings", {
      command(action: number, code: number) {
        if (action !== 18 || code !== 2) throw new Error("Unexpected order");
        w.sent += 1;
      },
    });
    w.aoeVassal.update(
      [12, 1, 0, 0, 0, 0, 77, 12, 13],
      "20/20 military\nholding territory\ndevelopment complete",
      "\n\nWatchTower at 92, 92",
      "Infantry · 6 holding\nArchers · 7 holding\nCavalry · 7 holding",
    );
  });
  await expect(page.locator("#vassal-status")).toContainText("20/20 military");
  const squads = page.getByRole("list", { name: "Defence squads" });
  await expect(squads.getByRole("listitem")).toHaveText([
    "Inf 6 hold",
    "Arc 7 hold",
    "Cav 7 hold",
  ]);
  await expect(squads.getByRole("listitem").first()).toHaveAttribute(
    "title",
    "Infantry · 6 holding",
  );
  await expect(page.locator("#vassal-status")).toContainText(
    "development complete",
  );
  await expect(
    page.getByLabel("Finished 77, Watch Tower at 92, 92"),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Stop development", exact: true })
    .click();
  expect(
    await page.evaluate(() => (window as unknown as { sent: number }).sent),
  ).toBe(1);
  const chooser = page.getByRole("combobox", { name: "Vassal" });
  await chooser.selectOption("13");
  await page.evaluate(() => {
    const { aoeVassal } = window as unknown as {
      aoeVassal: {
        selected(): number;
        update(v: number[], label: string, jobs: string, squads: string): void;
      };
    };
    aoeVassal.update(
      [aoeVassal.selected(), 0, 1, 1, 0, 1, 0, 12, 13],
      "20/20 military · UNDER ATTACK\ndevelopment paused",
      "\nRoad at 92, 92\n",
      "Infantry · 12 attacking, 8 moving",
    );
  });
  await expect(chooser).toHaveValue("13");
  await expect(squads.getByRole("listitem")).toHaveText([
    "Inf 12 attack, 8 mov",
  ]);
  await expect(
    page.getByRole("slider", { name: "Development priority" }),
  ).toHaveAttribute("aria-valuetext", "Roads");
  await expect(page.locator("#vassal-status")).toContainText("UNDER ATTACK");
  await expect(page.locator("#vassal-panel")).toHaveAttribute(
    "data-alert",
    "true",
  );
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
    await expect(page.locator("#vassal-status")).toContainText("/20 military", {
      timeout: 1_000,
    });
  }).toPass({ timeout: 20_000 });
  const panel = page.locator("#vassal-panel");
  await expect(panel.locator("[data-jobs=current]")).toContainText("Current");
  await expect(
    panel.getByRole("list", { name: "Defence squads" }),
  ).toContainText(/Inf \d+ (hold|mov|attack)/);
  const planned = panel.locator("[data-jobs=planned]");
  await expect(planned).toHaveText(/^Planned [1-9]\d*$/);
  await planned.hover();
  await expect(page.getByRole("tooltip")).toContainText(" at ");
  await expect(panel.locator("[data-jobs=finished]")).toContainText("Finished");
  await page.evaluate(() => {
    const { aoeVassal } = window as unknown as {
      aoeVassal: {
        update(
          values: number[],
          label: string,
          jobs: string,
          squads: string,
        ): void;
      };
    };
    const update = aoeVassal.update.bind(aoeVassal);
    aoeVassal.update = (values, label, jobs, squads) => {
      const panel = document.getElementById("vassal-panel");
      if (panel && values.length) {
        panel.dataset.serverTuning = `${values[3]}`;
      }
      update(values, label, jobs, squads);
    };
  });
  const focus = panel.getByRole("slider", { name: "Development priority" });
  await expect(focus).toHaveAccessibleDescription(
    "Choose what the Vassal builds first: defences or roads.",
  );
  await focus.press("End");
  await expect(focus).toHaveAttribute("aria-valuetext", "Roads");
  await focus.blur();
  await expect(panel).toHaveAttribute("data-server-tuning", "1", {
    timeout: 10_000,
  });
  await page
    .getByRole("button", { name: "Stop development", exact: true })
    .click();
  await expect(page.locator("#vassal-status")).toContainText(
    "development paused",
  );
  await page
    .getByRole("button", { name: "Resume development", exact: true })
    .click();
  await expect(page.locator("#vassal-status")).not.toContainText(
    "development paused",
  );
});

test("attacked Vassal shows its alert while a nearby General arrives and returns", async ({
  page,
  request,
}, testInfo) => {
  test.setTimeout(420_000);
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
    // The Vassal's walls make attackers path around closed gates, so the
    // General chases them along a long route. He has marched out once he is
    // far from home on screen, or has left the camera's view entirely.
    await expect(async () => {
      const general = await generalPosition().catch(() => undefined);
      if (!general) return;
      expect(
        Math.hypot(general.x - home.x, general.y - home.y),
      ).toBeGreaterThan(100);
    }).toPass({ timeout: 20_000 });
    await expect(page.locator("#vassal-status")).toContainText(
      "holding territory",
      { timeout: 150_000 },
    );
    await expect(async () => {
      const general = await generalPosition();
      expect(Math.abs(general.x - home.x)).toBeLessThan(4);
      expect(Math.abs(general.y - home.y)).toBeLessThan(4);
    }).toPass({ timeout: 90_000 });
  } finally {
    clearInterval(timer);
    await testInfo.attach("vassal-match-observations", {
      body: JSON.stringify(samples),
      contentType: "application/json",
    });
  }
});
