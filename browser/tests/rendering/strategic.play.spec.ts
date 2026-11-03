import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { forceCanvas, forceWebGl } from "./backends.js";

test("strategic tokens retain identity, layers and fog-safe reports", async ({
  page,
}) => {
  await page.route("**/strategic-test", (route) =>
    route.fulfill({
      contentType: "text/html",
      body: '<link rel="stylesheet" href="/interface/game.css"><canvas id="scene" style="width:1280px;height:720px"></canvas>',
    }),
  );
  await page.goto("/strategic-test");
  const result = await page.evaluate(async () => {
    // @ts-expect-error Served browser module, outside the tooling project.
    const { initStrategic } = await import("/navigation/strategic.js");
    const strategic = initStrategic(
      { disabled: true, canvas: document.createElement("canvas"), revision: 0 },
      { records: () => [], revision: () => 0 },
    );
    const jobs: number[] = [];
    [3, 4, 5, 6, 2].forEach((count, kind) => {
      for (let n = 0; n < count; n++)
        jobs.push(
          100 + jobs.length,
          31,
          kind < 4 ? kind : -1,
          kind === 4 ? 1 : 0,
          -1,
        );
    });
    strategic.receive(
      [
        7,
        500,
        500,
        1,
        1,
        42,
        1,
        0,
        520,
        500,
        20,
        10,
        6,
        4,
        2,
        8,
        530,
        500,
        0,
        0,
        -1,
        0,
        -1,
        NaN,
        NaN,
        0,
        0,
        0,
        0,
        0,
      ],
      [30, 480, 500, 0, 31, 25, 27, 4],
      [],
      [8, 530, 500],
      jobs,
      [7, 520, 500],
    );
    strategic.resources(new Float64Array([2, 510, 500, 42, 0]));
    const state = [16000, 16000, 500, 500, 1280, 720, 0.03, 2];
    strategic.frame(state);
    const pick = strategic.entity(640, 360);
    const unchanged = strategic.frame(state);
    const buttons = [...document.querySelectorAll(".army-token")].map(
      (e) => e.textContent,
    );
    const card = document.querySelector(".vassal-card")?.textContent;
    const bounds = [
      ...document.querySelectorAll(".army-token,.vassal-card"),
    ].map((e) => {
      const r = e.getBoundingClientRect();
      return { x: r.x, y: r.y, width: r.width, height: r.height };
    });
    const canvas =
      document.querySelector<HTMLCanvasElement>("#strategic-scene");
    const context = canvas?.getContext("2d");
    if (!canvas || !context) throw new Error("Strategic canvas missing");
    const gold = Array.from(context.getImageData(659, 370, 1, 1).data);
    strategic.resources(new Float64Array([]));
    strategic.frame(state);
    const retainedResources = canvas.dataset.resources;
    strategic.depleted(new Float64Array([42, 0]), true, false);
    strategic.resources(new Float64Array([2, 520, 500, 43, 0]));
    strategic.frame(state);
    const pendingResources = canvas.dataset.resources;
    strategic.depleted(new Float64Array([]), false, true);
    strategic.frame(state);
    const depletedResources = canvas.dataset.resources;
    strategic.update([], [], []);
    strategic.frame(state);
    const threats = canvas.dataset.threats;
    strategic.update([], [], [], [8, NaN, NaN]);
    strategic.frame(state);
    const deadThreats = canvas.dataset.threats;
    strategic.reset();
    strategic.frame(state);
    return {
      pick,
      unchanged,
      buttons,
      card,
      bounds,
      gold,
      retainedResources,
      pendingResources,
      depletedResources,
      threats,
      deadThreats,
      resetThreats: canvas.dataset.threats,
      remaining: document.querySelectorAll(".army-token,.vassal-card").length,
    };
  });
  expect(result.pick).toBe(7);
  expect(result.unchanged).toBe(false);
  expect(result.buttons[0]).toContain("42");
  expect(result.buttons[1]).toContain("?");
  expect(result.card).toContain("25");
  expect(result.card).toContain("Food shortage");
  expect(result.card).toContain("2 idle");
  expect(result.remaining).toBe(0);
  for (let i = 0; i < result.bounds.length; i++) {
    const a = result.bounds[i];
    if (!a) throw new Error("Token missing");
    for (const b of result.bounds.slice(i + 1)) {
      expect(
        a.x + a.width <= b.x ||
          b.x + b.width <= a.x ||
          a.y + a.height <= b.y ||
          b.y + b.height <= a.y,
      ).toBe(true);
    }
  }
  expect(result.gold).toEqual([242, 207, 80, 255]);
  expect(result.retainedResources).toBe("1");
  expect(result.pendingResources).toBe("1");
  expect(result.depletedResources).toBe("0");
  expect(result.threats).toBe("1");
  expect(result.deadThreats).toBe("0");
  expect(result.resetThreats).toBe("0");
  const armies = page.getByRole("button", { name: /Armies/ });
  await expect(armies).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press("F2");
  await expect(armies).toHaveAttribute("aria-pressed", "false");
  await armies.click();
  await expect(armies).toHaveAttribute("aria-pressed", "true");
});

test("a General is selected and marched without leaving far zoom", async ({
  page,
  request,
}, info) => {
  test.setTimeout(90_000);
  expect(
    (await request.post("/maps/reset?scenario=general-army")).status(),
  ).toBe(204);
  if (info.project.name === "canvas") await forceCanvas(page);
  else if (info.project.name !== "webgpu") await forceWebGl(page);
  await gameAssets(page);
  await page.goto("/?play=1#x=106&y=128&zoom=.5");
  await expect(page.locator("#connection")).toHaveText("connected");
  type Actor = { id: number; kind: string; x: number; y: number };
  await expect
    .poll(async () => {
      const actors: Actor[] = JSON.parse(
        (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
      );
      const castle = actors.find((a) => a.kind === "Castle");
      if (!castle) return false;
      await page.mouse.click(castle.x, castle.y);
      return page.locator("#train-general").isVisible();
    })
    .toBe(true);
  await page.locator("#train-general").click();
  await expect
    .poll(
      async () =>
        (await page.locator("#scene").getAttribute("data-actors"))?.includes(
          '"General"',
        ),
      { timeout: 30_000 },
    )
    .toBe(true);
  await page.evaluate(() => {
    location.hash = "x=112&y=128&zoom=.04";
  });
  await expect(page.locator("#scene")).toHaveAttribute("data-lod", "markers");
  const perfEnabled = () =>
    page.evaluate(
      () =>
        (window as Window & { aoePerformance?: { enabled: boolean } })
          .aoePerformance?.enabled,
    );
  expect(await perfEnabled()).toBe(false);
  await page.keyboard.press("F3");
  await expect(
    page.getByRole("button", { name: /Economy F3/ }),
  ).toHaveAttribute("aria-pressed", "false");
  expect(await perfEnabled()).toBe(false);
  await page.keyboard.press("F3");
  await page.keyboard.press("F8");
  expect(await perfEnabled()).toBe(true);
  await page.keyboard.press("F8");
  expect(await perfEnabled()).toBe(false);
  const token = page.locator(".army-token[data-owned='true']").first();
  await expect(token).toContainText("14");
  await token.click();
  await expect(page.locator("#selection")).toHaveAttribute(
    "data-kind",
    "General",
  );
  await expect(page.locator("#general-panel")).toBeVisible();
  await page.mouse.click(810, 380, { button: "right" });
  await expect(page.locator("#general-panel")).toHaveAttribute(
    "data-state",
    "marching",
  );
  await expect(page.locator("#scene")).toHaveAttribute("data-lod", "markers");
  await expect(token).toHaveAttribute("data-order", "marching");
  await page.evaluate(() => {
    location.hash = "x=112&y=128&zoom=.5";
  });
  await expect(page.locator("#scene")).toHaveAttribute("data-lod", "sprites");
  await expect(token).toBeHidden();
  await page.keyboard.press("F3");
  expect(await perfEnabled()).toBe(true);
  await page.keyboard.press("F3");
});
