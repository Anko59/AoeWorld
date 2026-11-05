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
    const cardNode = document.querySelector<HTMLElement>(".steward-card");
    const card = `${cardNode?.textContent} ${cardNode?.title}`;
    const bounds = [
      ...document.querySelectorAll(".army-token,.steward-card"),
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
      remaining: document.querySelectorAll(".army-token,.steward-card").length,
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
  await page.locator("#general-only-move").hover();
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

test("kingdom landscape keeps source relief, settlements and farms bounded and cached", async ({
  page,
}) => {
  await page.route("**/landscape-test", (route) =>
    route.fulfill({
      contentType: "text/html",
      body: '<link rel="stylesheet" href="/interface/game.css"><canvas id="scene" style="width:1280px;height:720px"></canvas><section id="bottom-panel"><div id="commands"></div><div id="selection" data-count="0"></div></section>',
    }),
  );
  await page.goto("/landscape-test");
  const result = await page.evaluate(async () => {
    // @ts-expect-error Served browser module.
    const { paintTerrain } = await import("/navigation/landscape/terrain.js");
    // @ts-expect-error Served browser module.
    const { initStrategic } = await import("/navigation/strategic.js");
    document.documentElement.dataset.interfaceArt = "original";
    const cells = new Uint8Array([
      0x20, 0xe0, 0x82, 0x89, 0x80, 0x80, 0x82, 0x89, 0x80, 0x80, 0x82, 0x89,
      0x80, 0x80, 0x82, 0x89,
    ]);
    const terrain = paintTerrain({ samples: 4, cells });
    const pixel = (x: number, y: number) =>
      Array.from(
        terrain.getContext("2d").getImageData(x, y, 1, 1).data,
      ) as number[];
    const dark = pixel(0, 0),
      lit = pixel(2, 0),
      forest = pixel(4, 0),
      river = pixel(6, 0);
    const s = initStrategic(
      { disabled: true, canvas: document.createElement("canvas"), revision: 0 },
      { records: () => [], revision: () => 0 },
    );
    s.terrain(terrain);
    const sites = [
      10, 498, 498, 502, 502, 0, 0x4191f5, 1, 11, 506, 500, 509, 503, 5,
      0x4191f5, 1, 12, 490, 500, 494, 504, 16, 0x4191f5, 0,
    ];
    s.settlements(sites);
    const state = [8000, 8000, 500, 500, 1280, 720, 0.04, 1];
    s.frame(state);
    const node = document.querySelector(
      '#strategic-settlements [data-kind="TownCenter"]',
    );
    const before = document
      .querySelector("#strategic-scene")
      ?.getAttribute("data-redraws");
    s.settlements(sites);
    s.frame(state);
    const cached =
      before ===
      document.querySelector("#strategic-scene")?.getAttribute("data-redraws");
    const retained =
      node ===
      document.querySelector('#strategic-settlements [data-kind="TownCenter"]');
    const count = document.querySelectorAll(
      "#strategic-settlements > *",
    ).length;
    const canvas =
      document.querySelector<HTMLCanvasElement>("#strategic-scene");
    if (!canvas) throw new Error("Missing strategic surface");
    const fields = canvas.dataset.fields;
    const owned = document.querySelectorAll(
      '#strategic-settlements [data-owned="true"]',
    ).length;
    const halo = Array.from(
      canvas.getContext("2d")?.getImageData(675, 360, 1, 1).data ?? [],
    );
    const many = Array.from({ length: 3000 }, (_, id) => {
      const x = (id % 50) * 150,
        y = Math.floor(id / 50) * 120;
      return [id, x, y, x + 4, y + 4, 1, 0x4191f5, 1];
    }).flat();
    s.settlements(many);
    s.terrain(
      paintTerrain({
        samples: 512,
        cells: new Uint8Array(512 * 512).fill(0x80),
      }),
    );
    s.frame([8000, 8000, 4000, 4000, 1280, 720, 0.001, 2]);
    const bounded = {
      icons: Number(canvas.dataset.settlements),
      borders: Number(canvas.dataset.borders),
    };
    const paintTimes: number[] = [],
      intervals: number[] = [];
    let last = performance.now();
    for (let i = 0; i < 35; i++) {
      s.frame([
        8000,
        8000,
        4000 + Math.sin(i / 10) * 80,
        4000,
        1280,
        720,
        0.001,
        2,
      ]);
      await new Promise(requestAnimationFrame);
      const now = performance.now();
      if (i >= 5) {
        paintTimes.push(Number(canvas.dataset.frameMs));
        intervals.push(now - last);
      }
      last = now;
    }
    const percentile = (v: number[], p: number) =>
      v.sort((a, b) => a - b)[Math.floor(v.length * p)] ?? 0;
    const budget = {
      inputFootprints: 3000,
      acceptedCap: 2048,
      iconCap: 192,
      terrainBytes: Number(canvas.dataset.terrainBytes),
      samples: paintTimes.length,
      paintP50Ms: percentile(paintTimes, 0.5),
      paintP95Ms: percentile(paintTimes, 0.95),
      frameP95Ms: percentile(intervals, 0.95),
    };
    s.reset();
    s.frame(state);
    return {
      backdrop: getComputedStyle(
        document.getElementById("bottom-panel") as HTMLElement,
      ).backgroundColor,
      dark,
      lit,
      forest,
      river,
      cached,
      retained,
      count,
      size: [terrain.width, terrain.height],
      fields,
      owned,
      halo,
      bounded,
      budget,
      remaining: document.querySelectorAll("#strategic-settlements > *").length,
    };
  });
  expect(result.backdrop).toBe("rgba(0, 0, 0, 0)");
  expect(result.size).toEqual([8, 8]);
  expect(result.lit[0]).toBeGreaterThan(result.dark[0] ?? -1);
  expect(result.forest[1]).toBeGreaterThan(result.forest[0] ?? -1);
  expect(result.river[2]).toBeGreaterThan(result.river[0] ?? -1);
  expect(result.cached).toBe(true);
  expect(result.retained).toBe(true);
  // The Castle's footprint overlaps the owned Town Center on screen, so it folds into one symbol.
  expect(result.count).toBe(1);
  expect(result.owned).toBe(1);
  expect(result.halo.slice(0, 3).every((n) => n > 180)).toBe(true);
  expect(result.fields).toBe("1");
  expect(result.bounded.icons).toBeLessThanOrEqual(192);
  expect(result.bounded.borders).toBeLessThanOrEqual(2048);
  expect(result.budget.terrainBytes).toBe(4 * 1024 * 1024);
  await test.info().attach("kingdom-render-budget", {
    body: JSON.stringify(result.budget, null, 2),
    contentType: "application/json",
  });
  expect(result.remaining).toBe(0);
});
