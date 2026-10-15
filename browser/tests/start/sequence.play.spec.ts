import { expect, test, type Page, type Route } from "@playwright/test";
import { gameAssets } from "../game-assets.js";

test.beforeEach(async ({ page, request }) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
});

test("the direct-game flag preserves terrain and capture hooks", async ({
  page,
}) => {
  await page.goto("/?play=1#x=100&y=100&zoom=1");
  await expect(page.locator("#scene")).toBeVisible();
  await expect(page.locator("#connection")).toHaveText("connected");
  const hash = process.env["AOE_E2E_LANDSCAPE_HASH"];
  if (!hash) throw new Error("The E2E stack must supply a prepared map");
  await page.evaluate(async (hash) => {
    const response = await fetch(`/maps/${hash}`, {
      method: "POST",
      headers: {
        "X-AoeWorld-Controller-Token":
          sessionStorage.getItem("aoeworld.resume-token") ?? "",
      },
    });
    if (!response.ok) throw new Error(await response.text());
  }, hash);
  await expect(page.locator("#connection")).toHaveText(/reconnect/);
  await page.getByRole("button", { name: "Reconnect to server" }).click();
  await expect(page.locator("#minimap")).toHaveAttribute("data-map-hash", hash);
  await page.evaluate(() => {
    location.hash = "x=100&y=101&zoom=1";
  });
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-chunks-pending",
    "0",
  );
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-raster",
    "ready",
  );
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-camera-x",
    "100",
  );
});

async function setup(page: Page) {
  await page.goto("/?skipIntro=1");
  await page
    .getByRole("button", { name: "Single Player", exact: true })
    .click();
  await expect(page.getByRole("heading", { name: "Game Setup" })).toBeVisible();
  await expect(page.locator("#connection")).toHaveText("connected");
}

test("intro can be skipped by a key and is remembered for this session", async ({
  page,
}) => {
  await page.goto("/?introStills=1");
  await expect(page.locator("#entry-intro")).toBeVisible();
  await page.keyboard.press("Space");
  await expect(page.locator("#main-menu")).toBeVisible();
  await page.reload();
  await expect(page.locator("#main-menu")).toBeVisible();
  await expect(page.locator("#entry-intro")).toBeHidden();
});

test("a click skips the intro and the URL flag opens the menu immediately", async ({
  page,
}) => {
  await page.goto("/?introStills=1");
  await page.locator("#entry-intro").click();
  await expect(page.locator("#main-menu")).toBeVisible();
  await page.evaluate(() => sessionStorage.clear());
  await page.goto("/?skipIntro=1");
  await expect(page.locator("#main-menu")).toBeVisible();
  await expect(page.locator("#entry-intro")).toBeHidden();
});

test("single player opens skirmish with eight accessible player colours", async ({
  page,
}) => {
  await setup(page);
  const colours = page.getByRole("radiogroup", { name: "Player colour" });
  await expect(colours.getByRole("radio")).toHaveCount(8);
  await colours.getByRole("radio", { name: "Purple", exact: true }).check();
  await expect(
    colours.getByRole("radio", { name: "Purple", exact: true }),
  ).toBeChecked();
  await page.reload();
  await page
    .getByRole("button", { name: "Single Player", exact: true })
    .click();
  await expect(
    colours.getByRole("radio", { name: "Purple", exact: true }),
  ).toBeChecked();
  await page.getByRole("button", { name: "Back to menu" }).click();
  for (const name of ["Settings", "Multiplayer", "Campaign"]) {
    await expect(
      page.getByRole("button", { name: new RegExp(name) }),
    ).toBeDisabled();
  }
});

test("a preset submits the creator request and loading waits for visible terrain", async ({
  page,
}) => {
  await setup(page);
  await page.getByRole("radio", { name: "Orange", exact: true }).check();
  await page.getByRole("button", { name: "Alps", exact: true }).click();
  await expect(page.getByLabel("Starting latitude")).toHaveValue("46.6");
  await expect(page.getByLabel("Starting longitude")).toHaveValue("10.5");
  let original: Record<string, unknown> = {};
  let completed = false;
  await page.route("**/maps/jobs", async (route) => {
    if (route.request().method() === "GET") return route.fulfill({ json: [] });
    original = route.request().postDataJSON();
    expect(route.request().headers()["idempotency-key"]).toMatch(
      /^[0-9a-f]{32}$/,
    );
    await route.fulfill({ json: { id: 91 } });
  });
  const hash = process.env["AOE_E2E_LANDSCAPE_HASH"];
  if (!hash) throw new Error("The E2E stack must supply a prepared map");
  let rasterRoute: Route | undefined;
  await page.route(`**/maps/${hash}/minimap`, (route) => {
    rasterRoute = route;
  });
  await page.route("**/maps/jobs/91", async (route) => {
    await route.fulfill({
      json: {
        id: 91,
        request: original,
        state: completed ? "completed" : "running",
        content_hash: hash,
        stage: "preparing_source",
        preparation: {
          mode: "overview",
          explanation: "Preparing the landscape",
        },
        progress: {
          phase: "building_pyramids",
          completed: 4,
          total: 16,
          unit: "pages",
        },
      },
    });
  });
  await page.getByRole("button", { name: "Start Game", exact: true }).click();
  await expect(page.locator("#loading-screen")).toBeVisible();
  await expect(page.locator("#loading-phase")).toContainText("4 / 16 pages");
  await expect(page.getByRole("progressbar")).toHaveAttribute(
    "aria-valuenow",
    "4",
  );
  await expect(page.getByRole("progressbar")).toHaveAttribute(
    "aria-valuemax",
    "16",
  );
  expect(original).toMatchObject({
    schema_version: 1,
    center_latitude_e7: 466000000,
    center_longitude_e7: 105000000,
    requested_side_meters: 30000,
    compression: { numerator: 30, denominator: 1 },
    year_ce: 600,
    seed: 1,
    reconstruction_profile: "circa600_v1",
    preparation: "automatic",
    hydrology_mode: "automatic",
  });
  // Hold the actual minimap request to prove job completion alone cannot
  // uncover an unfinished scene. Activation and terrain are real.
  completed = true;
  await expect.poll(() => Boolean(rasterRoute)).toBe(true);
  await expect(page.locator("#loading-phase")).toContainText("terrain");
  await expect(page.locator("#loading-screen")).toBeVisible();
  if (!rasterRoute) throw new Error("minimap request is missing");
  // Activation can put the map centre over water, away from the starting unit.
  // Move the camera off the start while loading to exercise that case on the
  // prepared fixture, whose safe start otherwise happens to be its centre.
  await page.evaluate(() => {
    location.hash = "x=40&y=40&zoom=1";
  });
  await expect(page.locator("#minimap")).toHaveAttribute("data-camera-x", "40");
  await rasterRoute.continue();
  await expect(page.locator("#start-sequence")).toBeHidden();
  await expect(page.locator("#scene")).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(() => {
        const minimap = document.getElementById("minimap");
        const primary = document.getElementById("minimap-primary");
        const x = Number(minimap?.dataset["cameraX"]);
        const y = Number(minimap?.dataset["cameraY"]);
        const px = Number(primary?.getAttribute("cx")) * 2;
        const py = Number(primary?.getAttribute("cy")) * 2;
        return Math.hypot(x - px, y - py);
      }),
    )
    .toBeLessThan(0.1);
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-chunks-pending",
    "0",
  );
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-player-colour",
    "orange",
  );
  await expect(page.locator("#minimap-primary")).toHaveAttribute(
    "fill",
    "#ff8000",
  );
  await page.getByRole("button", { name: "Open map creator" }).click();
  await expect(page.getByLabel("Region", { exact: true })).toHaveValue("alps");
  await expect(page.locator("#map-location")).toContainText("Alps");
});

test("preparation errors explain the failure and Back restores the setup choices", async ({
  page,
}) => {
  await setup(page);
  await page
    .getByRole("button", { name: "Brittany coast", exact: true })
    .click();
  let original: Record<string, unknown> = {};
  await page.route("**/maps/jobs", async (route) => {
    if (route.request().method() === "GET") return route.fulfill({ json: [] });
    original = route.request().postDataJSON();
    await route.fulfill({ json: { id: 92 } });
  });
  await page.route("**/maps/jobs/92", async (route) => {
    await route.fulfill({
      json: {
        id: 92,
        state: "failed",
        request: original,
        preparation: { mode: "overview" },
        error: "Elevation source is unavailable.",
      },
    });
  });
  await page.getByRole("button", { name: "Start Game", exact: true }).click();
  await expect(page.locator("#loading-error")).toContainText(
    "Elevation source is unavailable",
  );
  await page.getByRole("button", { name: "Back to setup" }).click();
  await expect(page.getByRole("heading", { name: "Game Setup" })).toBeVisible();
  await expect(page.getByLabel("Starting latitude")).toHaveValue("48.39");
});

test("a real fallback activation error allows a fresh map request after Back", async ({
  page,
}) => {
  await setup(page);
  const requests: Record<string, unknown>[] = [];
  page.on("request", (request) => {
    if (
      request.method() === "POST" &&
      new URL(request.url()).pathname === "/maps/jobs"
    ) {
      requests.push(request.postDataJSON());
    }
  });
  await page.getByRole("button", { name: "Start Game", exact: true }).click();
  await expect(page.locator("#loading-error")).toContainText(
    "safe starting place",
  );
  await page.getByRole("button", { name: "Back to setup" }).click();
  await page.getByRole("button", { name: "Alps", exact: true }).click();
  await page.getByRole("button", { name: "Start Game", exact: true }).click();
  await expect.poll(() => requests.length).toBe(2);
  expect(requests[0]?.["center_latitude_e7"]).toBe(488566000);
  expect(requests[1]?.["center_latitude_e7"]).toBe(466000000);
});
