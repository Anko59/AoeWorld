// Opt-in real-art capture. The ordinary village regression uses attack.ts.
import { expect, type Page } from "@playwright/test";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { test } from "../../../server.js";
import { centerVillage, freshHud } from "../fixtures/resources.js";

type Actor = {
  id: number;
  kind: string;
  x: number;
  y: number;
  head: number;
  hp: number;
  maxHp: number;
};
type Step = Record<string, unknown>;
async function actors(page: Page): Promise<Actor[]> {
  return JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as Actor[];
}

test("capture real Feudal military and prepare its narrated showcase", async ({
  page,
  request,
}, info) => {
  const config = await readFile("../.cache/military-capture.json", "utf8")
    .then((s) => JSON.parse(s) as { url: string })
    .catch(() => null);
  test.skip(
    !config || info.project.name === "canvas",
    "Opt-in native WebGPU capture",
  );
  if (!config || config.url !== "http://127.0.0.1:8098/")
    throw new Error("Military capture uses this checkout's port 8098");
  test.setTimeout(180_000);
  await page.setViewportSize({ width: 1280, height: 720 });
  expect(
    (
      await request.post(`${config.url}maps/reset?scenario=feudal-military`)
    ).status(),
  ).toBe(204);
  await page.goto(`${config.url}?play=1`);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await centerVillage(page, true);
  await page.mouse.move(640, 360);
  await page.mouse.wheel(0, 288);
  await freshHud(page);
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-village-art",
    "native",
  );
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    "webgpu",
  );
  await mkdir("../.cache/military-shots", { recursive: true });
  const frameCount = Number(
    await page.locator("#playground").getAttribute("data-atlas-frames"),
  );
  expect(frameCount).toBeGreaterThan(0);
  expect(frameCount).toBeLessThanOrEqual(5120);
  await writeFile(
    "../.cache/military-shots/atlas.json",
    JSON.stringify({
      frameCount,
      pages: 5,
      atlasBytes: 83886080,
      art: "native",
      renderer: "webgpu",
    }),
  );
  const steps: Step[] = [
    { wait_for: { state: "connected" } },
    { wait_for: { selector: '#scene[data-village-art="native"]' } },
    { key: "h" },
    { wheel: { selector: "#scene", x: 640, y: 360, delta_y: 288 } },
    { wait_for: { selector: '#selection-name:text-is("Town Center")' } },
  ];
  const select = async (kind: string, nth = 0) => {
    await freshHud(page);
    const a = (await actors(page)).filter(
      (a) => a.kind === kind && a.x > 0 && a.x < 1280 && a.y > 54 && a.y < 512,
    )[nth];
    if (!a) throw new Error(`Visible ${kind} ${nth} missing`);
    const y = a.head < a.y ? a.head + 10 : a.y;
    await page.mouse.click(a.x, y);
    await expect(page.locator("#selection")).toHaveAttribute("data-kind", kind);
    steps.push({
      click_at: { selector: "#scene", x: Math.round(a.x), y: Math.round(y) },
    });
  };
  const click = async (selector: string) => {
    await page.locator(selector).click();
    steps.push({ click: selector });
  };
  const wait = async (selector: string) => {
    await expect(page.locator(selector)).toBeVisible({ timeout: 60_000 });
    steps.push({ wait_for: { selector } });
  };
  const initial = await actors(page);
  const enemyMilitia = initial.find((a) => a.kind === "Militia");
  const enemyScout = initial.filter((a) => a.kind === "Scout").at(-1);
  if (!enemyMilitia || !enemyScout) throw new Error("Military targets missing");
  await click("#research-loom");
  await select("ArcheryRange");
  for (let i = 0; i < 2; i++) await click("#train-archer");
  await click("#train-skirmisher");
  await select("Barracks");
  await click("#train-spearman");
  await click("#train-spearman");
  await select("Stable");
  await click("#train-scout");
  await page.screenshot({ path: "../.cache/military-shots/training.png" });
  await select("Blacksmith");
  await click("#research-fletching");
  await wait('#research-status:has-text("Fletching")');
  await page.screenshot({ path: "../.cache/military-shots/fletching.png" });
  await wait("#research-forging:enabled");
  await click("#research-forging");
  await wait('#research-status:has-text("Forging")');
  await wait("#research-scale-mail-armor:enabled");
  await wait('#scene[data-actors*="Skirmisher"]');
  await select("Skirmisher");
  await expect(page.locator("#unit-combat")).toContainText("Range 5");
  const moveSelected = async (x: number, y: number) => {
    await click("#move-units");
    await page.mouse.click(x, y, { button: "right" });
    steps.push({ click_at: { selector: "#scene", x, y, button: "right" } });
  };
  // Ordinary move commands bring the trained army in front of the roofs.
  await moveSelected(180, 430);
  for (const [kind, nth, x, y] of [
    ["Scout", 2, 550, 455],
    ["Archer", 0, 390, 360],
    ["Archer", 1, 450, 410],
    ["Spearman", 0, 360, 415],
    ["Spearman", 1, 410, 425],
    ["Villager", 0, 275, 370],
  ] as const) {
    await select(kind, nth);
    await moveSelected(x, y);
  }
  steps.push({ wait_ms: 5000 });
  await page.waitForTimeout(5000);
  const zoom = async (delta: number) => {
    await page.mouse.move(350, 400);
    await page.mouse.wheel(0, delta);
    await freshHud(page);
    steps.push({
      wheel: { selector: "#scene", x: 350, y: 400, delta_y: delta },
    });
  };
  await zoom(-400);
  await select("Skirmisher");
  await page.screenshot({ path: "../.cache/military-shots/army.png" });
  await writeFile(
    "../.cache/military-shots/actors.json",
    JSON.stringify(await actors(page), null, 2),
  );
  const order = async (id: number) => {
    await freshHud(page);
    const target = (await actors(page)).find((a) => a.id === id);
    if (!target) throw new Error(`Target ${id} missing`);
    const y = target.head < target.y ? target.head + 10 : target.y;
    await page.mouse.click(target.x, y, { button: "right" });
    steps.push({
      click_at: {
        selector: "#scene",
        x: Math.round(target.x),
        y: Math.round(y),
        button: "right",
      },
    });
  };
  // Loom protects the villager while it holds the enemy in the open. Once
  // retaliation starts, both targets stand still for the recorded clicks.
  await select("Villager", 0);
  await order(enemyMilitia.id);
  await select("Militia");
  await wait('#selection-task:text-is("Attacking")');
  await select("Archer", 0);
  await order(enemyMilitia.id);
  await expect
    .poll(
      async () =>
        Number(await page.locator("#scene").getAttribute("data-projectiles")),
      { timeout: 20_000, intervals: [25] },
    )
    .toBeGreaterThan(0);
  await page.screenshot({ path: "../.cache/military-shots/arrows.png" });
  await select("Archer", 1);
  await order(enemyMilitia.id);
  steps.push({ wait_ms: 2500 });
  await page.waitForTimeout(2500);
  await select("Spearman", 0);
  await order(enemyScout.id);
  await select("Spearman", 1);
  await order(enemyScout.id);
  steps.push({ wait_ms: 350 });
  await page.waitForTimeout(350);
  await page.screenshot({
    path: "../.cache/military-shots/spearmen-fight.png",
  });
  await expect
    .poll(
      async () =>
        (await actors(page)).find((a) => a.id === enemyScout.id)?.hp ?? 0,
      { timeout: 20_000 },
    )
    .toBe(0);
  steps.push({ wait_ms: 4000 });
  await page.screenshot({ path: "../.cache/military-shots/spearmen-win.png" });
  await zoom(400);
  await select("Blacksmith");
  await page.screenshot({ path: "../.cache/military-shots/blacksmith.png" });
  expect(steps.length).toBeLessThanOrEqual(64);
  await writeFile(
    "../.cache/military-storyboard.json",
    JSON.stringify(
      {
        title: "Feudal military",
        voice: "Kore",
        renderer: "webgpu",
        scenes: [
          {
            kind: "browser",
            path: "/?play=1",
            caption: "Feudal military · real art and commands · 4× local clock",
            seconds: 4,
            steps,
            narration:
              "The Feudal Age now has an army. The Archery Range trains Archers and Skirmishers, the Barracks trains Spearmen, and the Stable trains the same Scout Cavalry that starts the game. At the Blacksmith, Fletching improves ranged attack and range; Forging strengthens melee attacks. A villager with Loom holds the Militia while Archers shoot visible arrows with travel time. Spearmen counter cavalry with their attack bonus. This local demonstration uses real game art and ordinary commands, with game time accelerated four times.",
          },
        ],
      },
      null,
      2,
    ),
  );
  await page.close();
  expect(
    (
      await request.post(`${config.url}maps/reset?scenario=feudal-military`)
    ).status(),
  ).toBe(204);
});
