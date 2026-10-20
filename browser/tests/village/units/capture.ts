import { expect, type Page } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";
import { centerVillage, freshHud } from "../resources.js";

type Actor = {
  id: number;
  kind: string;
  x: number;
  y: number;
  hp: number;
  task: string;
};
const actors = async (page: Page) =>
  JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as Actor[];
async function actor(page: Page, kind: string, id?: number) {
  await freshHud(page);
  const found = (await actors(page)).find(
    (a) => a.kind === kind && a.hp > 0 && (id === undefined || a.id === id),
  );
  if (!found) throw new Error(`Capture ${kind} missing`);
  return found;
}
async function click(page: Page, a: Actor, right = false) {
  await page.mouse.click(a.x, a.y, { button: right ? "right" : "left" });
}
async function zoom(page: Page, value = 0.25) {
  await page.evaluate((z) => {
    location.hash = `zoom=${z}`;
  }, value);
  await freshHud(page);
}
async function build(page: Page, kind: string) {
  await centerVillage(page);
  await zoom(page, 0.35);
  await click(page, await actor(page, "Villager"));
  await page.locator("#build-menu").click();
  await page.locator(`#build-${kind.toLowerCase()}`).click();
  let placed = false;
  for (const [x, y] of [
    [400, 290],
    [850, 290],
    [420, 430],
    [850, 430],
    [640, 180],
  ] as const) {
    await page.mouse.move(x, y);
    await freshHud(page);
    if (
      (await page.locator("#scene").getAttribute("data-placement-valid")) !==
      "true"
    )
      continue;
    await page.mouse.click(x, y);
    placed = true;
    break;
  }
  if (!placed) throw new Error(`No capture placement for ${kind}`);
  await expect
    .poll(async () => (await actors(page)).some((a) => a.kind === kind))
    .toBe(true);
  await click(page, await actor(page, kind));
  await expect(page.locator("#selection-name")).toHaveText(kind);
  await expect(page.locator("#building-status")).toContainText("Complete", {
    timeout: 90_000,
  });
}
async function prepare(page: Page, url: string) {
  expect((await page.request.get(`${url}asset-pack/manifest.json`)).ok()).toBe(
    true,
  );
  expect((await page.request.post(`${url}maps/reset`)).status()).toBe(204);
  await page.goto(`${url}?play=1`);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-village-art",
    "native",
  );
  await centerVillage(page);
  await zoom(page);
  const scout = await actor(page, "Scout");
  await click(page, scout);
  // Claim the sheep before returning the scout out of the way.
  const sheep = await actor(page, "Sheep");
  await page.locator("#move-units").click();
  await click(page, sheep, true);
  await page.waitForTimeout(6500);
  await page.mouse.click(704, 296, { button: "right" });
  await build(page, "Barracks");
  await build(page, "House");
  await centerVillage(page);
  await zoom(page);
  await click(page, await actor(page, "Barracks"));
  await expect(page.locator("#selection-name")).toHaveText("Barracks");
  await page.keyboard.press("q");
  await expect(page.locator("#training-label")).toContainText("Militia");
  await expect
    .poll(async () => (await actors(page)).some((a) => a.kind === "Militia"), {
      timeout: 35_000,
    })
    .toBe(true);
  const militia = await actor(page, "Militia");
  await click(page, militia);
  await page.mouse.click(848, 464, { button: "right" });
  await expect(page.locator("#selection-task")).toHaveText("Moving");
  await expect
    .poll(async () => (await actor(page, "Militia", militia.id)).task, {
      timeout: 45_000,
    })
    .toBe("Idle");
  await freshHud(page);
  return {
    militia: await actor(page, "Militia"),
    barracks: await actor(page, "Barracks"),
    wolf: await actor(page, "Wolf"),
    deer: await actor(page, "Deer"),
    villager: await actor(page, "Villager", 3),
    sheep: await actor(page, "Sheep"),
    boar: await actor(page, "Boar"),
  };
}
const stepClick = (a: { x: number; y: number }, right = false) => ({
  click_at: {
    selector: "#scene",
    x: Math.round(a.x),
    y: Math.round(a.y),
    ...(right ? { button: "right" } : {}),
  },
});
const opening = [
  { wait_for: { state: "connected" } },
  { wait_for: { selector: '#population:text-matches("^[0-9]+/[0-9]+$")' } },
  { key: "h" },
  { key: "Escape" },
  { wheel: { selector: "#scene", x: 640, y: 360, delta_y: 2000 } },
  { wait_for: { selector: '#minimap[data-zoom="0.25"]' } },
];

const focusStep = (a: Actor) => ({
  wheel: {
    selector: "#scene",
    x: Math.round(a.x),
    y: Math.round(a.y),
    delta_y: -900,
  },
});

async function shot(page: Page, name: string, kind: string, id?: number) {
  const focus = await actor(page, kind, id);
  const camera = await page.locator("#minimap").evaluate((m) => ({
    x: Number(m.getAttribute("data-camera-x")),
    y: Number(m.getAttribute("data-camera-y")),
    zoom: Number(m.getAttribute("data-zoom")),
  }));
  const dx = (focus.x - 640) / (64 * camera.zoom),
    dy = (focus.y - 360) / (32 * camera.zoom);
  await page.evaluate(
    ({ x, y }) => {
      location.hash = `x=${x}&y=${y}&zoom=1.4`;
    },
    { x: camera.x + (dx + dy) / 2, y: camera.y + (dy - dx) / 2 },
  );
  await freshHud(page);
  await freshHud(page);
  await page.screenshot({ path: `../.cache/units-shots/${name}.png` });
  await page.evaluate((c) => {
    location.hash = `x=${c.x}&y=${c.y}&zoom=${c.zoom}`;
  }, camera);
  await freshHud(page);
  await freshHud(page);
}

/** Opt-in, real native art and server gameplay; no injected state or wire. */
export async function captureUnits(page: Page, url: string) {
  await page.setViewportSize({ width: 1280, height: 720 });
  await mkdir("../.cache/units-shots", { recursive: true });
  await mkdir("../.cache/showcase", { recursive: true });
  const start = await prepare(page, url);
  await page.screenshot({ path: "../.cache/units-shots/animals.png" });
  await shot(page, "sheep", "Sheep");
  await shot(page, "deer", "Deer");
  await shot(page, "wolf", "Wolf");
  await shot(page, "boar", "Boar");
  await click(page, start.militia);
  await click(page, start.wolf, true);
  await expect(page.locator("#selection-task")).toContainText("Attacking");
  await expect
    .poll(async () => (await actor(page, "Militia")).hp, {
      timeout: 15_000,
    })
    .toBeLessThan(40);
  await shot(page, "militia-attacking", "Militia");
  await expect
    .poll(
      async () => (await actors(page)).find((a) => a.id === start.wolf.id)?.hp,
      { timeout: 35_000 },
    )
    .toBe(0);
  const afterWolf = await actor(page, "Militia");
  await click(page, await actor(page, "Villager", 3));
  await click(page, await actor(page, "Deer"), true);
  await expect(page.locator("#selection-task")).toContainText("Hunting");
  await expect
    .poll(
      async () => (await actors(page)).find((a) => a.id === start.deer.id)?.hp,
      { timeout: 40_000 },
    )
    .toBeLessThan(5);
  await shot(page, "hunter", "Villager", 3);
  await expect
    .poll(
      async () => (await actors(page)).find((a) => a.id === start.deer.id)?.hp,
      {
        timeout: 40_000,
      },
    )
    .toBe(0);
  await expect(page.locator("#selection-load")).toContainText("food", {
    timeout: 20_000,
  });
  await shot(page, "butchering", "Villager", 3);
  await click(page, await actor(page, "Sheep"));
  await page.mouse.click(512, 296, { button: "right" });
  await page.waitForTimeout(1500);
  await shot(page, "herding", "Sheep");
  await click(page, await actor(page, "Militia"));
  await click(page, await actor(page, "Boar"), true);
  await expect
    .poll(
      async () =>
        (await actors(page)).find((a) => a.id === start.militia.id)?.hp,
      { timeout: 45_000 },
    )
    .toBe(0);
  await page.waitForTimeout(2500);
  const dead = (await actors(page)).find((a) => a.id === start.militia.id);
  if (!dead) throw new Error("Corpse vanished before its decay capture");
  const camera = await page.locator("#minimap").evaluate((m) => ({
    x: Number(m.getAttribute("data-camera-x")),
    y: Number(m.getAttribute("data-camera-y")),
    zoom: Number(m.getAttribute("data-zoom")),
  }));
  const dx = (dead.x - 640) / (64 * camera.zoom),
    dy = (dead.y - 360) / (32 * camera.zoom);
  await page.evaluate(
    ({ x, y }) => {
      location.hash = `x=${x}&y=${y}&zoom=1.4`;
    },
    { x: camera.x + (dx + dy) / 2, y: camera.y + (dy - dx) / 2 },
  );
  await freshHud(page);
  await freshHud(page);
  await page.screenshot({ path: "../.cache/units-shots/corpse.png" });
  const fresh = await prepare(page, url);
  await writeFile(
    "../.cache/units-shots/points.json",
    JSON.stringify({ fresh, afterWolf }, null, 2),
  );
  const storyboard = {
    title: "Dark Age units and wildlife",
    voice: "Kore",
    scenes: [
      {
        kind: "browser",
        path: "/?play=1",
        caption: "Train Militia at the Barracks",
        seconds: 23,
        narration:
          "The Barracks trains a Militia. Units use their own native Dark Age animations and player colour.",
        steps: [
          ...opening,
          stepClick(fresh.barracks),
          { key: "q" },
          { wait_for: { selector: '#training-label:has-text("Militia")' } },
          focusStep(fresh.barracks),
        ],
      },
      {
        kind: "browser",
        path: "/?play=1",
        caption: "Militia attack a wolf",
        seconds: 4,
        narration:
          "Right click a wolf to attack. The Militia closes the distance, strikes, and takes damage from the wolf's retaliation.",
        steps: [
          ...opening,
          stepClick(fresh.militia),
          stepClick(fresh.wolf, true),
          { wait_for: { selector: '#selection-task:text-is("Attacking")' } },
          focusStep(fresh.wolf),
          {
            wait_for: {
              selector: '#selection-task:text-is("Idle")',
              timeout_ms: 45_000,
            },
          },
        ],
      },
      {
        kind: "browser",
        path: "/?play=1",
        caption: "A villager hunts and butchers a deer",
        seconds: 10,
        narration:
          "Villagers hunt deer with a bow, then butcher the carcass and carry the food home.",
        steps: [
          ...opening,
          stepClick(fresh.villager),
          stepClick(fresh.deer, true),
          { wait_for: { selector: '#selection-task:text-is("Hunting")' } },
          focusStep(fresh.deer),
          {
            wait_for: {
              selector: '#selection-load:has-text("food")',
              timeout_ms: 60_000,
            },
          },
        ],
      },
      {
        kind: "browser",
        path: "/?play=1",
        caption: "Owned sheep can be herded",
        seconds: 7,
        narration:
          "Owned sheep wear the player's colour. Select one and right click to herd it to a new position.",
        steps: [
          ...opening,
          stepClick(fresh.sheep),
          stepClick({ x: 512, y: 296 }, true),
          focusStep(fresh.sheep),
        ],
      },
      {
        kind: "browser",
        path: "/?play=1",
        caption: "Death, corpse and thirty seconds of decay",
        seconds: 4,
        narration:
          "The wounded Militia attacks a boar and dies. Its death animation plays once, then its corpse decays for thirty seconds and disappears. These timings and hit points come from the server.",
        steps: [
          ...opening,
          stepClick(afterWolf),
          stepClick(fresh.boar, true),
          focusStep(fresh.boar),
          {
            wait_for: {
              selector: '#selection-task:text-is("Dying")',
              timeout_ms: 45_000,
            },
          },
          {
            wait_for: {
              selector: "#selection-empty:not([hidden])",
              timeout_ms: 40_000,
            },
          },
        ],
      },
    ],
  };
  await writeFile(
    "../.cache/showcase/units.json",
    JSON.stringify(storyboard, null, 2),
  );
}
