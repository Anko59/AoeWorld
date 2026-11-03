import { expect, type Page } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";
import { centerVillage, freshHud } from "../../village/fixtures/resources.js";

type Actor = { id: number; kind: string; x: number; y: number; owned: boolean };

/** Licensed-pack inspection through ordinary research and construction orders. */
export async function imperialArtProof(
  page: Page,
  url: string,
  startup: boolean,
) {
  expect(
    (
      await page.request.post(`${url}maps/reset?scenario=imperial-army`)
    ).status(),
  ).toBe(204);
  await page.goto(`${url}?play=1#zoom=.4&renderer=webgpu`);
  const scene = page.locator("#scene");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await expect(scene).toHaveAttribute("data-village-art", "native", {
    timeout: 60_000,
  });
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    "webgpu",
  );
  await expect(scene).toHaveAttribute("data-chunks-pending", "0", {
    timeout: 60_000,
  });
  await expect(scene).toHaveAttribute("data-chunks-failed", "0");
  for (const id of [
    103, 217, 264, 429, 237, 209, 265, 428, 96, 255, 257, 75, 77, 80, 201, 219,
    12, 221, 279,
  ]) {
    await expect(
      page
        .locator(
          `[data-ui-group="technologies"][data-ui-id="${id}"] .original-icon`,
        )
        .first(),
    ).toBeAttached();
  }
  for (const id of [473, 567, 359, 492, 283, 569, 441, 422, 548, 550]) {
    const catalog = await page.request.get(
      `${url}asset-pack/buildings/interface.json`,
    );
    expect((await catalog.json()).units[id]).toBeDefined();
  }
  await mkdir("../.cache/imperial-art/shots", { recursive: true });
  const actors = async (): Promise<Actor[]> =>
    JSON.parse((await scene.getAttribute("data-actors")) ?? "[]");
  const frame = async (kind: string, zoom: number) => {
    await centerVillage(page);
    await page.evaluate(() => {
      location.hash = "zoom=.35";
    });
    await freshHud(page);
    const actor = (await actors()).find((a) => a.owned && a.kind === kind);
    if (!actor) throw new Error(`Missing ${kind}`);
    const camera = await page.locator("#minimap").evaluate((el) => ({
      x: Number(el.dataset["cameraX"]),
      y: Number(el.dataset["cameraY"]),
      zoom: Number(el.dataset["zoom"]),
    }));
    const dx = (actor.x - 640) / (64 * camera.zoom),
      dy = (actor.y - 360) / (32 * camera.zoom);
    const x = camera.x + (dx + dy) / 2,
      y = camera.y + (dy - dx) / 2;
    await page.evaluate(
      ({ x, y, zoom }) => {
        location.hash = `x=${x}&y=${y}&zoom=${zoom}`;
      },
      { x, y, zoom },
    );
    await expect(page.locator("#minimap")).toHaveAttribute(
      "data-zoom",
      String(zoom),
    );
    await expect(scene).toHaveAttribute("data-chunks-pending", "0");
    await freshHud(page);
    const framed = (await actors()).find((a) => a.id === actor.id);
    if (!framed) throw new Error(`${kind} left the captured scene`);
    return framed;
  };
  const select = async (kind: string) => {
    const actor = await frame(kind, 0.5);
    await page.mouse.click(actor.x, actor.y);
    await expect(page.locator("#selection")).toHaveAttribute("data-kind", kind);
  };
  const capture = async (name: string, kind: string, zoom = 2) => {
    const actor = await frame(kind, zoom);
    await page.mouse.click(actor.x, actor.y);
    await expect(page.locator("#selection")).toHaveAttribute("data-kind", kind);
    await expect(
      page.locator("#selection-portrait .original-icon"),
    ).toBeVisible();
    await expect(scene).toHaveAttribute("data-chunks-failed", "0");
    // Hold the inspected native animation in the recording as well as the still.
    await page.waitForTimeout(1200);
    await page.screenshot({ path: `../.cache/imperial-art/shots/${name}.png` });
    await writeFile(
      `../.cache/imperial-art/shots/${name}.json`,
      JSON.stringify(
        {
          health: await (await page.request.get(`${url}health`)).json(),
          age: await page.locator("#age").textContent(),
          actors: await actors(),
          camera: await page
            .locator("#minimap")
            .evaluate((el) => ({ ...el.dataset })),
        },
        null,
        2,
      ),
    );
  };
  await capture("unit-pack-startup", "Knight");
  if (startup) return;
  const build = async (kind: string, command: string, military: boolean) => {
    await select("Villager");
    await page
      .locator(military ? "#build-military-menu" : "#build-menu")
      .click();
    await page.locator(`#build-${command}`).click();
    let placed = false;
    for (const y of [180, 260, 340, 420]) {
      for (const x of [300, 450, 800, 950, 1100]) {
        await page.mouse.move(x, y);
        await freshHud(page);
        if ((await scene.getAttribute("data-placement-valid")) !== "true")
          continue;
        await page.mouse.click(x, y);
        placed = true;
        break;
      }
      if (placed) break;
    }
    expect(placed).toBe(true);
    await expect
      .poll(async () =>
        (await actors()).some((a) => a.owned && a.kind === kind),
      )
      .toBe(true);
    await select(kind);
    await expect(page.locator("#building-status")).toHaveText("Complete", {
      timeout: 40_000,
    });
  };
  await build("Castle", "castle", true);
  if (!(await actors()).some((a) => a.owned && a.kind === "House"))
    await build("House", "house", false);
  for (const kind of ["House", "Barracks", "Castle", "TownCenter"])
    await capture(`age3-${kind}`, kind, 0.8);
  await select("Castle");
  await page.locator("#train-vassal").click();
  await page.locator("#train-general").click();
  await select("TownCenter");
  await page.locator("#research-imperial-age").click();
  await expect(page.locator("#age")).toContainText("Imperial Age", {
    timeout: 25_000,
  });
  for (const kind of ["House", "Barracks", "Castle", "TownCenter"])
    await capture(`age4-${kind}`, kind, 0.8);
  const research = async (
    site: string,
    tech: string,
    resultingUnit: string,
  ) => {
    await select(site);
    await page.screenshot({
      path: `../.cache/imperial-art/shots/menu-${tech}.png`,
    });
    await page.locator(`#research-${tech}`).click();
    await expect
      .poll(
        async () =>
          (await actors()).some((a) => a.owned && a.kind === resultingUnit),
        { timeout: 25_000 },
      )
      .toBe(true);
    await capture(resultingUnit, resultingUnit);
  };
  await research("Barracks", "two-handed-swordsman", "TwoHandedSwordsman");
  await research("Stable", "cavalier", "Cavalier");
  await research("SiegeWorkshop", "capped-ram", "CappedRam");
  for (const [site, tech, kind] of [
    ["Barracks", "champion", "Champion"],
    ["Barracks", "halberdier", "Halberdier"],
    ["ArcheryRange", "arbalester", "Arbalester"],
    ["Stable", "paladin", "Paladin"],
    ["Stable", "light-cavalry", "LightCavalry"],
    ["Stable", "hussar", "Hussar"],
    ["SiegeWorkshop", "siege-ram", "SiegeRam"],
    ["SiegeWorkshop", "onager", "Onager"],
  ]) {
    if (!site || !tech || !kind) throw new Error("Incomplete research capture");
    await research(site, tech, kind);
  }
  for (const kind of ["Vassal", "General"]) await capture(kind, kind);
}
