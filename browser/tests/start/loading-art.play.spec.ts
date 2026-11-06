import { expect, type Page } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";

test.beforeEach(async ({ page, request }) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
});

// A one-pixel JPEG stands in for the owner's local paintings (never in Git).
const pixel = Buffer.from(
  "/9j/4AAQSkZJRgABAQEASABIAAD/2wBDAP//////////////////////////////////////////////////////////////////////////////////////wgALCAABAAEBAREA/8QAFBABAAAAAAAAAAAAAAAAAAAAAP/aAAgBAQABPxA=",
  "base64",
);
const manifest = {
  version: 1,
  series: [
    {
      id: "01-kings-with-globes",
      name: "Kings with globes",
      images: ["a", "b"].map((name) => ({
        file: `01-kings-with-globes/${name}.jpg`,
        culture: name.toUpperCase(),
        width: 1,
        height: 1,
      })),
    },
    {
      id: "02-war-councils",
      name: "War councils",
      images: ["c", "d", "e"].map((name) => ({
        file: `02-war-councils/${name}.jpg`,
        culture: name.toUpperCase(),
        width: 1,
        height: 1,
      })),
    },
  ],
};

async function serveArt(page: Page, present: boolean) {
  await page.route("**/series-art/**", (route) => {
    const path = new URL(route.request().url()).pathname;
    if (!present) return route.fulfill({ status: 404, body: "missing" });
    if (path.endsWith("manifest.json"))
      return route.fulfill({ json: manifest });
    return route.fulfill({ contentType: "image/jpeg", body: pixel });
  });
}

async function startLoading(page: Page) {
  await page.route("**/maps/jobs", (route) => {
    if (route.request().method() === "GET") return route.fulfill({ json: [] });
    return route.fulfill({ json: { id: 95 } });
  });
  await page.route("**/maps/jobs/95", (route) =>
    route.fulfill({
      json: {
        id: 95,
        state: "running",
        stage: "preparing_overview",
        progress: {
          phase: "building_pyramids",
          completed: 2,
          total: 8,
          unit: "pages",
        },
      },
    }),
  );
  await page.goto("/?skipIntro=1");
  await page
    .getByRole("button", { name: "Single Player", exact: true })
    .click();
  await expect(page.locator("#connection")).toHaveText("connected");
  await page.getByRole("button", { name: "Start Game", exact: true }).click();
  await expect(page.locator("#loading-screen")).toBeVisible();
}

test("the menu shows a king painting from the local series", async ({
  page,
}) => {
  await serveArt(page, true);
  await page.goto("/?skipIntro=1");
  await expect(page.locator("#main-menu")).toBeVisible();
  await expect(page.locator("#main-menu")).not.toHaveClass(/art-pending/);
  const art = await page
    .locator("#main-menu")
    .evaluate((node) => node.style.getPropertyValue("--menu-art"));
  expect(art).toMatch(/\/series-art\/01-kings-with-globes\/[ab]\.jpg/);
});

test("the menu and loading screen keep the bundled art when the series is absent", async ({
  page,
}) => {
  await serveArt(page, false);
  await page.goto("/?skipIntro=1");
  await expect(page.locator("#main-menu")).not.toHaveClass(/art-pending/);
  expect(
    await page
      .locator("#main-menu")
      .evaluate((node) => node.style.getPropertyValue("--menu-art")),
  ).toBe("");
  await startLoading(page);
  await expect(page.locator("#loading-art")).toBeHidden();
});

test("loading shows one random series, changing painting after five seconds", async ({
  page,
}) => {
  await serveArt(page, true);
  await startLoading(page);
  const layer = page.locator("#loading-art");
  await expect(layer).toHaveAttribute("data-image", /series-art/);
  const series = await layer.getAttribute("data-series");
  const first = (await layer.getAttribute("data-image")) ?? "";
  const shownAt = Date.now();
  await page.waitForTimeout(3500);
  expect(await layer.getAttribute("data-image")).toBe(first);
  await expect(layer).not.toHaveAttribute("data-image", first, {
    timeout: 5000,
  });
  expect(Date.now() - shownAt).toBeGreaterThan(4500);
  expect(await layer.getAttribute("data-series")).toBe(series);
  await expect(page.locator(".loading-art-slide.visible")).toHaveCount(1);
});

test("the loading bar is one monotonic 0-100 bar with a converging estimate", async ({
  page,
}) => {
  await page.goto("/?skipIntro=1");
  const result = await page.evaluate(async () => {
    const modelUrl = "/start/progress-model.js";
    const { createProgressModel, STAGES, formatRemaining } = await import(
      modelUrl
    );
    const memory = new Map<string, string>();
    const storage = {
      getItem: (key: string) => memory.get(key) ?? null,
      setItem: (key: string, value: string) => void memory.set(key, value),
    };
    const simulate = (speed: number, skip: string[]) => {
      let clock = 0;
      const model = createProgressModel({
        now: () => clock,
        storage,
        request: { size: 480, opponents: 7, signature: "s" },
      });
      const plan = (STAGES as [string, number][])
        .map(([key, ms]) => [key, ms * speed] as const)
        .filter(([key]) => !skip.includes(key));
      const total = plan.reduce((sum: number, [, ms]) => sum + ms, 0);
      let last = 0;
      let backwards = 0;
      let stalled = 0;
      let moved = clock;
      let late = 0;
      let lateCount = 0;
      while (clock < total) {
        let start = 0;
        let at = 0;
        while (at < plan.length - 1 && clock >= start + (plan[at]?.[1] ?? 0))
          start += plan[at++]?.[1] ?? 0;
        if (clock % 250 < 33) model.update(plan[at]?.[0], null, null);
        const { fraction, etaMs } = model.sample();
        if (fraction < last) backwards++;
        if (fraction > last) moved = clock;
        if (clock - moved > 1000) stalled++;
        last = fraction;
        if (clock > total / 2) {
          late += Math.abs(etaMs - (total - clock));
          lateCount++;
        }
        clock += 33;
      }
      model.finish();
      return {
        backwards,
        stalled,
        last,
        lateError: late / lateCount / total,
        text: formatRemaining(75_000),
      };
    };
    return [
      simulate(1, []),
      simulate(3, []),
      simulate(1, ["downloading_source", "sampling_water"]),
    ];
  });
  for (const [at, run] of result.entries()) {
    expect(run.backwards).toBe(0);
    expect(run.stalled).toBe(0);
    expect(run.last).toBeLessThan(1);
    // A skipped stage is unknowable in advance: its run only has to behave.
    if (at < 2) expect(run.lateError).toBeLessThan(0.25);
  }
  expect(result[0]?.text).toBe("about 1 min 20 s");
});

test("intro shots follow the clip length and leave no frozen frame", async ({
  page,
}) => {
  await page.goto("/?skipIntro=1");
  const plan = await page.evaluate(async () => {
    const url = "/start/cinematic.js";
    const { shotSchedule } = await import(url);
    return shotSchedule([3000, 3000, 3000, 3000]);
  });
  expect(plan.starts).toEqual([0, 2300, 4600, 6900]);
  // Each next shot starts before the clip before it ends.
  plan.starts
    .slice(1)
    .forEach((start: number, at: number) =>
      expect(start).toBeLessThan(plan.starts[at] + 3000),
    );
  expect(plan.end).toBe(9900);
  expect(plan.reveal).toBeGreaterThan(plan.end);
});
