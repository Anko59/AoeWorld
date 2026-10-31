import { expect, type Page } from "@playwright/test";
import { mkdir, readFile } from "node:fs/promises";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { centerVillage, freshHud } from "../fixtures/resources.js";
import { forceCanvas, forceWebGl } from "../../rendering/backends.js";

test.use({ video: "on" });

type Actor = {
  id: number;
  kind: string;
  x: number;
  y: number;
  selected: boolean;
};
const actors = async (page: Page): Promise<Actor[]> =>
  JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  );
async function select(page: Page, kind: string, excludedId?: number) {
  let attempt = 0;
  await expect(async () => {
    await freshHud(page);
    const candidates = (await actors(page)).filter(
      (a) => a.kind === kind && a.id !== excludedId,
    );
    const actor = candidates[attempt++ % candidates.length];
    if (!actor) throw new Error(`${kind} has no target`);
    await page.mouse.click(actor.x, actor.y - (kind === "Villager" ? 8 : 0));
    // Same-kind selections leave the label unchanged. Wait for the actor
    // snapshot produced after this click before saving a control group.
    await freshHud(page);
    await expect(page.locator("#selection-name")).toHaveText(kind, {
      timeout: 500,
    });
    expect(
      (await actors(page)).filter((a) => a.selected).map((a) => a.id),
    ).toEqual([actor.id]);
  }).toPass({ timeout: 20_000, intervals: [500] });
}

async function centerCastle(page: Page) {
  await page.evaluate(() => {
    history.replaceState(null, "", location.pathname + location.search);
    location.hash = "x=141&y=136&zoom=.5";
  });
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-camera-x",
    "141",
  );
  await freshHud(page);
}

test("train one Vassal per Castle and activate its visible economic domain", async ({
  page,
  request,
}, info) => {
  test.setTimeout(150_000);
  if (info.project.name === "canvas") await forceCanvas(page);
  if (info.project.name === "firefox-webgl2") await forceWebGl(page);
  // Optional capture uses only this checkout's own make dev DEV_PORT=8148.
  const capture =
    info.project.name === "firefox-webgl2"
      ? await readFile("../.cache/vassal/capture.json", "utf8")
          .then((s) => JSON.parse(s) as { url: string })
          .catch(() => null)
      : null;
  if (capture && capture.url !== "http://127.0.0.1:8148/")
    throw new Error("Unexpected Vassal capture origin");
  const base = capture?.url ?? "/";
  if (capture) await mkdir("../.cache/vassal", { recursive: true });
  expect(
    (await request.post(`${base}maps/reset?scenario=vassal-economy`)).status(),
  ).toBe(204);
  if (!capture || !(await request.get(`${base}asset-pack/manifest.json`)).ok())
    await gameAssets(page, true);
  await page.goto(`${base}?play=1`);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await centerVillage(page, true);
  // Retain one idle worker before automation starts. Once managed, farmers
  // move between HUD reads and pointer events; a click at an old position
  // selects the Farm instead. Recall its identity through the normal UI.
  await select(page, "Villager");
  await freshHud(page);
  const previous = (await actors(page)).find((a) => a.selected);
  if (!previous) throw new Error("Initial villager selection missing");
  // A retry can switch between villagers while the HUD still says Villager.
  // Exercise that same-kind transition before recording the group's identity.
  await select(page, "Villager", previous.id);
  await expect(page.locator("#selection-task")).toHaveText("Idle");
  const workerIds = (await actors(page))
    .filter((a) => a.selected)
    .map((a) => a.id);
  expect(workerIds).toHaveLength(1);
  await page.keyboard.press("Control+1");
  // Check the saved identity before automation or footprint movement can act.
  await page.keyboard.press("Escape");
  await page.keyboard.press("1");
  await freshHud(page);
  await expect
    .poll(async () =>
      (await actors(page)).filter((a) => a.selected).map((a) => a.id),
    )
    .toEqual(workerIds);
  await centerCastle(page);
  await select(page, "Castle");
  const train = page.locator("#train-vassal");
  await expect(train).toBeEnabled();
  await expect(train).toHaveAttribute("title", /1000 food.*1000 gold/);
  const food = Number(await page.locator("#stockpile-food").textContent());
  const gold = Number(await page.locator("#stockpile-gold").textContent());
  await train.click();
  await expect(page.locator("#stockpile-food")).toHaveText(String(food - 1000));
  await expect(page.locator("#stockpile-gold")).toHaveText(String(gold - 1000));
  await expect(train).toBeDisabled();
  await expect
    .poll(async () => (await actors(page)).some((a) => a.kind === "Vassal"), {
      timeout: 35_000,
    })
    .toBe(true);
  await select(page, "Vassal");
  await expect(page.locator("#vassal-status")).toContainText("home Castle");
  const viewport = page.viewportSize();
  if (!viewport) throw new Error("Viewport missing");
  // Bring the King out from behind its Castle using an ordinary leashed move.
  await page.mouse.click(viewport.width / 2, viewport.height / 2 + 120, {
    button: "right",
  });
  await expect
    .poll(
      async () => (await actors(page)).find((a) => a.kind === "Vassal")?.y ?? 0,
    )
    .toBeGreaterThan(viewport.height / 2 + 70);
  await expect(page.locator("#selection-task")).toHaveText("Idle");
  if (capture)
    await page.screenshot({ path: "../.cache/vassal/king-firefox.png" });
  await page.locator("#garrison").click();
  const castle = (await actors(page)).find((a) => a.kind === "Castle");
  if (!castle) throw new Error("Castle missing");
  await page.mouse.click(castle.x, castle.y, { button: "right" });
  await expect
    .poll(async () => (await actors(page)).some((a) => a.kind === "Vassal"), {
      timeout: 20_000,
    })
    .toBe(false);
  await select(page, "Castle");
  await expect(page.locator("#garrison-count")).toHaveText("1 / 20");
  await expect(train).toBeDisabled();
  await expect(page.locator("#vassal-domain")).toHaveAttribute(
    "data-active",
    "true",
  );
  await expect(page.locator("#vassal-domain")).toHaveAttribute(
    "data-radius",
    "100",
  );
  await expect(page.locator("#vassal-status")).toContainText("domain active");
  await centerVillage(page);
  await page.keyboard.press("1");
  await expect(page.locator("#selection-name")).toHaveText("Villager");
  await expect
    .poll(async () =>
      (await actors(page)).filter((a) => a.selected).map((a) => a.id),
    )
    .toEqual(workerIds);
  await expect(page.locator("#vassal-status")).toHaveText("Managed by Vassal");
  if (capture)
    await page.screenshot({ path: "../.cache/vassal/managed-firefox.png" });
  await centerCastle(page);
  await select(page, "Castle");
  const size = page.viewportSize();
  if (!size) throw new Error("Viewport missing");
  const ring = page.locator("#vassal-domain polygon");
  await expect(ring).toBeVisible();
  const points = ((await ring.getAttribute("points"))?.split(" ") ?? []).map(
    (point) => point.split(",").map(Number),
  );
  expect(points).toHaveLength(64);
  expect(points.every((p) => p.length === 2 && p.every(Number.isFinite))).toBe(
    true,
  );
  // Independent projection check: opposing x-axis points span 200 tiles,
  // each 64 CSS pixels at zoom 1, hence 6400 pixels at the normal .5 zoom.
  expect(
    Math.abs(Number(points[0]?.[0]) - Number(points[32]?.[0])),
  ).toBeCloseTo(6400, 0);
  if (capture) {
    // The circle exceeds the viewport. Pan to its southeast edge for the image.
    await page.evaluate(() => {
      location.hash = "x=211&y=206&zoom=.5";
    });
    await expect(page.locator("#minimap")).toHaveAttribute(
      "data-camera-x",
      "211",
    );
    await freshHud(page);
    const edge = (await ring.getAttribute("points"))?.split(" ") ?? [];
    expect(
      edge.some((point) => {
        const [x = NaN, y = NaN] = point.split(",").map(Number);
        return x > 0 && x < size.width && y > 0 && y < size.height - 200;
      }),
    ).toBe(true);
    await page.screenshot({ path: "../.cache/vassal/domain-firefox.png" });
    await centerCastle(page);
  }
  await page.locator("#ungarrison").click();
  await expect(page.locator("#garrison-count")).toHaveText("0 / 20");
  await expect(page.locator("#vassal-domain")).toHaveAttribute(
    "data-active",
    "false",
  );
  await expect(train).toBeDisabled();
});
