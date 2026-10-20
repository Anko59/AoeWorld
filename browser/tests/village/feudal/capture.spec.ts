// Opt-in native-art evidence and a reproducible storyboard, driven by the UI.
import { expect, type Page } from "@playwright/test";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { test } from "../../../server.js";
import { centerVillage } from "../fixtures/resources.js";

type Actor = { kind: string; x: number; y: number };
type Step = Record<string, unknown>;
async function actors(page: Page): Promise<Actor[]> {
  return JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as Actor[];
}

test("capture the native Feudal economy and prepare the live showcase", async ({
  page,
  request,
}) => {
  const config = await readFile("../.cache/feudal-capture.json", "utf8")
    .then((s) => JSON.parse(s) as { url: string })
    .catch(() => null);
  test.skip(!config, "Local Feudal capture was not requested");
  if (!config || config.url !== "http://127.0.0.1:18086/")
    throw new Error("Feudal capture must use this worktree's port 18086");
  test.setTimeout(180_000);
  await page.setViewportSize({ width: 1280, height: 720 });
  expect(
    (
      await request.post(`${config.url}maps/reset?scenario=feudal-economy`)
    ).status(),
  ).toBe(204);
  await page.goto(`${config.url}?play=1`);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-fog-disabled",
    "false",
  );
  await centerVillage(page, true);
  await page.mouse.move(640, 360);
  await page.mouse.wheel(0, 288);
  await expect
    .poll(async () =>
      Number(await page.locator("#minimap").getAttribute("data-zoom")),
    )
    .toBeCloseTo(Math.exp(-288 * 0.0015), 3);
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-village-art",
    "native",
  );
  await mkdir("../.cache/feudal-shots", { recursive: true });
  const steps: Step[] = [
    { wait_for: { state: "connected" } },
    { key: "h" },
    { wait_for: { selector: '#selection-name:text-is("Town Center")' } },
    { wheel: { selector: "#scene", x: 640, y: 360, delta_y: 288 } },
    { wait_ms: 500 },
  ];
  const click = async (selector: string) => {
    await page.locator(selector).click();
    steps.push({ click: selector });
  };
  const wait = async (selector: string) => {
    await expect(page.locator(selector)).toBeVisible({ timeout: 30_000 });
    steps.push({ wait_for: { selector } });
  };
  const actor = async (kind: string) => {
    const value = (await actors(page)).find((a) => a.kind === kind);
    if (!value) throw new Error(`${kind} missing`);
    await page.mouse.click(value.x, value.y);
    steps.push({
      click_at: {
        selector: "#scene",
        x: Math.round(value.x),
        y: Math.round(value.y),
      },
    });
  };
  await page.screenshot({
    path: "../.cache/feudal-shots/rebased-dark-age.png",
  });
  await click("#research-feudal-age");
  await wait('#age:text-is("II  Feudal Age")');
  await page.screenshot({ path: "../.cache/feudal-shots/rebased-age-up.png" });

  const locations: Array<readonly [number, number]> = [];
  for (const [id, kind] of [
    ["archery-range", "ArcheryRange"],
    ["blacksmith", "Blacksmith"],
    ["market", "Market"],
    ["watch-tower", "WatchTower"],
  ] as const) {
    await actor("Villager");
    await click(
      id === "archery-range" || id === "watch-tower"
        ? "#build-military-menu"
        : "#build-menu",
    );
    await click(`#build-${id}`);
    let placed = false;
    for (const [x, y] of [
      [1000, 270],
      [320, 340],
      [650, 390],
      [940, 420],
      [440, 230],
      [820, 220],
      [400, 410],
      [800, 420],
    ] as const) {
      if (locations.some(([a, b]) => Math.abs(a - x) + Math.abs(b - y) < 80))
        continue;
      await page.mouse.move(x, y);
      await page.waitForTimeout(180);
      if (
        (await page.locator("#scene").getAttribute("data-placement-valid")) !==
        "true"
      )
        continue;
      await page.mouse.click(x, y);
      steps.push({ click_at: { selector: "#scene", x, y } });
      locations.push([x, y]);
      placed = true;
      break;
    }
    expect(placed, `place ${kind}`).toBe(true);
    await wait("#construction-labels span");
    await actor(kind);
    await wait('#building-status:text-is("Complete")');
    await page.screenshot({ path: `../.cache/feudal-shots/rebased-${id}.png` });
  }
  for (const [kind, tech] of [
    ["Mill", "horse-collar"],
    ["TownCenter", "wheelbarrow"],
  ] as const) {
    await actor(kind);
    await click(`#research-${tech}`);
    await wait("#research-status:not(:empty)");
    await expect(page.locator("#research-status")).toHaveText("", {
      timeout: 20_000,
    });
    steps.push({
      wait_for: { selector: "#building-panel:has(#research-status:empty)" },
    });
  }
  await page.keyboard.press("Escape");
  steps.push({ key: "Escape" });
  await page.screenshot({
    path: "../.cache/feudal-shots/rebased-feudal-village.png",
  });
  expect(steps.length).toBeLessThanOrEqual(64);
  await writeFile(
    "../.cache/feudal-storyboard.json",
    JSON.stringify(
      {
        title: "Feudal Age: buildings and economy",
        scenes: [
          {
            kind: "browser",
            path: "/?play=1",
            caption: "Feudal economy · real commands · 20× local demo",
            seconds: 4,
            steps,
            narration:
              "The village reaches the Feudal Age, and its buildings take their new appearance. Villagers construct an Archery Range, a Blacksmith, a Market and a Watch Tower. Horse Collar gives newly placed farms more food, and Wheelbarrow improves villager speed and carrying. This local demonstration accelerates game time twenty times; market trading and Feudal military rosters follow separately.",
          },
        ],
      },
      null,
      2,
    ),
  );
  await page.close();
  // A fresh service has no controller lease: the recorder gets the first one.
  expect(
    (
      await request.post(`${config.url}maps/reset?scenario=feudal-economy`)
    ).status(),
  ).toBe(204);
});
