import { expect, type Page } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";

// What the entry sequence downloads, and when. The game module is fetched and
// compiled from the first script on, behind the cinematic or the resume probe;
// the client itself still starts only once that is over. The cinematic's
// posters are fetched when it plays, not by a visitor who has already seen it.
const MODULE = "/pkg/aoe_client_bg.wasm";
const POSTERS = [
  "shot1-king",
  "shot2-ship",
  "shot3-cavalry",
  "shot4-castle",
].map((shot) => `/art/cinematic/${shot}-poster.jpg`);

test.beforeEach(async ({ page, request }) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
});

/** The page's requests for any of `paths`, in the order issued. */
function requested(page: Page, paths: readonly string[]): string[] {
  const issued: string[] = [];
  page.on("request", (request) => {
    const path = new URL(request.url()).pathname;
    if (paths.includes(path)) issued.push(path);
  });
  return issued;
}

/** Stops the page's timers, so that only a key press ends the cinematic. */
async function holdCinematic(page: Page) {
  const epoch = Date.now();
  await page.clock.install({ time: epoch });
  await page.clock.pauseAt(epoch + 1000);
}

test("a first visit requests the game module once, behind the opening cinematic", async ({
  page,
}) => {
  test.setTimeout(60_000);
  await holdCinematic(page);
  const modules = requested(page, [MODULE]);
  await page.goto("/?introStills=1");
  await expect(page.locator("#entry-intro")).toBeVisible();
  await expect
    .poll(() => modules, { message: "module requests behind the cinematic" })
    .toEqual([MODULE]);
  await expect(page.locator("body")).toHaveAttribute(
    "data-start-state",
    "intro",
  );
  await page.keyboard.press("Space");
  await page.clock.resume();
  await expect(page.locator("#main-menu")).toBeVisible();
  // The game starts from that download: nothing fetches the module again.
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
  expect(modules).toEqual([MODULE]);
});

test("the module is requested before the resume decision and handed over compiled", async ({
  page,
}) => {
  await page.addInitScript(() => {
    localStorage.setItem("aoeworld.resume-token", "a".repeat(48));
    Reflect.set(window, "starts", []);
  });
  // Stands in for the client: the entry state and the module it starts with.
  await page.route("**/pkg/aoe_client.js", (route) =>
    route.fulfill({
      contentType: "application/javascript",
      body: `export default async function(input) {
        const state = document.body.dataset.startState;
        const compiled = await input?.module_or_path instanceof WebAssembly.Module;
        window.starts.push({ state, compiled });
      }`,
    }),
  );
  let release!: () => void;
  const decision = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route("**/skirmish", async (route) => {
    await decision;
    await route.fulfill({ json: { phase: "idle", ai_available: true } });
  });
  const modules = requested(page, [MODULE]);
  const starts = () => page.evaluate(() => Reflect.get(window, "starts"));
  await page.goto("/?introStills=1");
  await expect(page.locator("body")).toHaveAttribute(
    "data-start-state",
    "loading",
  );
  await expect
    .poll(() => modules, { message: "module requests before the decision" })
    .toEqual([MODULE]);
  expect(await starts()).toEqual([]);
  release();
  await expect(page.locator("#entry-intro")).toBeVisible();
  await page.keyboard.press("Space");
  await expect.poll(starts).toEqual([{ state: "menu", compiled: true }]);
  expect(modules).toEqual([MODULE]);
});

test("a returning visitor requests no cinematic poster", async ({ page }) => {
  const posters = requested(page, POSTERS);
  await page.goto("/?introStills=1");
  // A first visit plays the cinematic over its four posters.
  const stills = page.locator(".intro-shot img");
  await expect(stills).toHaveCount(4);
  for (const still of await stills.all())
    await expect(still).toHaveJSProperty("naturalWidth", 1280);
  expect([...new Set(posters)].sort()).toEqual(POSTERS);
  await page.keyboard.press("Space");
  await expect(page.locator("#main-menu")).toBeVisible();
  const firstVisit = posters.length;
  await page.reload();
  await expect(page.locator("#main-menu")).toBeVisible();
  await expect(page.locator("#entry-intro")).toBeHidden();
  expect(posters.slice(firstVisit)).toEqual([]);
});

test("a failed module download is reported like a failed start, never uncaught", async ({
  page,
}) => {
  await holdCinematic(page);
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.route(`**${MODULE}`, (route) =>
    route.fulfill({ status: 503, body: "unavailable" }),
  );
  await page.goto("/?introStills=1");
  await expect(page.locator("#entry-intro")).toBeVisible();
  // A download begun behind the cinematic has failed by the time this settles.
  await page.waitForLoadState("networkidle");
  await page.keyboard.press("Space");
  await expect(page.locator("#main-menu")).toBeVisible();
  await expect(page.locator("#connection")).toHaveText("Unavailable");
  await expect(page.locator("#unsupported")).toContainText(
    "Unable to start AoeWorld",
  );
  expect(errors).toEqual([]);
});

test("a browser without streaming compilation starts from one module request", async ({
  page,
}) => {
  test.setTimeout(60_000);
  await page.addInitScript(() => {
    Reflect.deleteProperty(WebAssembly, "compileStreaming");
    Reflect.deleteProperty(WebAssembly, "instantiateStreaming");
  });
  const modules = requested(page, [MODULE]);
  await page.goto("/?skipIntro=1");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
  expect(modules).toEqual([MODULE]);
});
