import { expect, type Page } from "@playwright/test";
import { writeFile } from "node:fs/promises";
import { centerVillage, freshHud } from "../village/fixtures/resources.js";

/** Optional private proof: ordinary commands against this checkout's dev server. */
export async function castleProof(
  page: Page,
  url: string,
  renderer: "webgpu" | "webgl2",
  startupOnly: boolean,
  resume = false,
  townCenterOnly = false,
) {
  if (!resume) {
    expect(
      (
        await page.request.post(`${url}maps/reset?scenario=feudal-military`)
      ).status(),
    ).toBe(204);
  }
  await page.goto(`${url}?play=1#renderer=${renderer}`);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await expect(page.locator("html")).toHaveAttribute(
    "data-interface-art",
    "original",
  );
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-village-art",
    "native",
  );
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    renderer,
  );
  const actors = async () =>
    JSON.parse(
      (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
    ) as {
      id: number;
      kind: string;
      x: number;
      y: number;
      owned: boolean;
      hp: number;
      maxHp: number;
    }[];
  const select = async (kind: string) => {
    const actor = (await actors()).find((a) => a.kind === kind);
    if (!actor) throw new Error(`${kind} missing`);
    await page.mouse.click(actor.x, actor.y);
    await expect(page.locator("#selection")).toHaveAttribute("data-kind", kind);
  };
  const capture = async (name: string) => {
    await freshHud(page);
    const browser = page.context().browser()?.browserType().name();
    const path = `../.cache/ui/castle-${name}-${browser}`;
    await page.screenshot({ path: `${path}.png` });
    await writeFile(
      `${path}.json`,
      JSON.stringify(
        {
          url,
          health: await (await page.request.get(`${url}health`)).json(),
          age: await page.locator("#age").textContent(),
          selection: await page.locator("#selection").innerText(),
          renderer: await page
            .locator("#playground")
            .getAttribute("data-renderer"),
          atlasFrames: await page
            .locator("#playground")
            .getAttribute("data-atlas-frames"),
          actors: await actors(),
        },
        null,
        2,
      ),
    );
  };
  await centerVillage(page, true);
  // The native 128-pixel tiles put Range exit cells above the viewport at 1x.
  // Use the ordinary camera link so every trained actor can be clicked.
  await page.evaluate((renderer) => {
    location.hash = `zoom=0.5&renderer=${renderer}`;
  }, renderer);
  await expect(page.locator("#minimap")).toHaveAttribute("data-zoom", "0.5");
  await freshHud(page);
  const stage = async (kind: string, x: number) => {
    await select(kind);
    const before = (await actors()).find((a) => a.kind === kind);
    if (!before) throw new Error(`${kind} missing before movement`);
    await page.mouse.click(x, 480, { button: "right" });
    await expect
      .poll(
        async () => {
          const actor = (await actors()).find((a) => a.id === before.id);
          return actor ? Math.hypot(actor.x - before.x, actor.y - before.y) : 0;
        },
        { timeout: 20_000 },
      )
      .toBeGreaterThan(30);
    await expect(page.locator("#unit-state")).toHaveText("unit idle", {
      timeout: 20_000,
    });
  };
  if (startupOnly && resume)
    await expect(page.locator("#age")).toContainText("Castle Age");
  await capture(`startup-${renderer}`);
  if (startupOnly) return;
  if (!resume) {
    await page.locator("#research-castle-age").click();
    await expect(page.locator("#age")).toContainText("Castle Age", {
      timeout: 50_000,
    });
    await capture("town-center");
    if (townCenterOnly) return;
    await select("Stable");
    await page.locator("#train-knight").click();
    await expect
      .poll(async () => (await actors()).some((a) => a.kind === "Knight"), {
        timeout: 15_000,
      })
      .toBe(true);
    await stage("Knight", 1000);
    await capture("knight");
    await select("Villager");
    await page.locator("#build-military-menu").click();
    await capture("commands");
    await page.locator("#build-castle").click();
    let placed = false;
    for (const [x, y] of [
      [920, 360],
      [400, 240],
      [420, 420],
      [850, 420],
      [640, 180],
    ]) {
      if (x === undefined || y === undefined)
        throw new Error("Placement point missing");
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
    expect(placed).toBe(true);
    await expect
      .poll(async () => (await actors()).some((a) => a.kind === "Castle"))
      .toBe(true);
    await select("Castle");
    await expect(page.locator("#building-status")).toHaveText("Complete", {
      timeout: 80_000,
    });
    await expect(page.locator("#building-hp")).toContainText("4800");
    await capture("building");

    await select("ArcheryRange");
    await page.locator("#research-crossbowman").click();
    await expect(page.locator("#train-crossbowman")).toBeEnabled({
      timeout: 20_000,
    });
    await page.locator("#train-crossbowman").click();
    await expect
      .poll(
        async () => (await actors()).some((a) => a.kind === "Crossbowman"),
        {
          timeout: 20_000,
        },
      )
      .toBe(true);
    await stage("Crossbowman", 1060);
    await capture("crossbowman");
  } else {
    await expect(page.locator("#age")).toContainText("Castle Age");
    await expect
      .poll(async () => (await actors()).some((a) => a.kind === "Castle"))
      .toBe(true);
  }

  const build = async (kind: string, command: string, military: boolean) => {
    await select("Villager");
    await page
      .locator(military ? "#build-military-menu" : "#build-menu")
      .click();
    await page.locator(`#build-${command}`).click();
    let placed = false;
    const sites =
      kind === "Monastery"
        ? [
            [820, 450],
            [1040, 240],
            [750, 470],
          ]
        : [
            [650, 450],
            [700, 480],
            [900, 220],
          ];
    for (const [x, y] of [
      ...sites,
      [400, 240],
      [420, 420],
      [850, 420],
      [640, 180],
      [920, 360],
      [1040, 240],
      [260, 360],
    ]) {
      if (x === undefined || y === undefined) continue;
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
    expect(placed).toBe(true);
    await expect
      .poll(async () => (await actors()).some((a) => a.kind === kind))
      .toBe(true);
    await select(kind);
    await expect(page.locator("#building-status")).toHaveText("Complete", {
      timeout: 40_000,
    });
  };
  await build("SiegeWorkshop", "siege-workshop", true);
  await page.locator("#train-mangonel").click();
  await expect
    .poll(async () => (await actors()).some((a) => a.kind === "Mangonel"), {
      timeout: 20_000,
    })
    .toBe(true);
  await stage("Mangonel", 1120);
  await capture("mangonel");
  await build("Monastery", "monastery", false);
  await page.locator("#train-monk").click();
  await expect
    .poll(async () => (await actors()).some((a) => a.kind === "Monk"), {
      timeout: 20_000,
    })
    .toBe(true);
  await select("Monk");
  await capture("monk");
  const initialMonk = (await actors()).find((a) => a.kind === "Monk");
  if (!initialMonk) throw new Error("Monk missing before healing proof");
  await page.mouse.click(1080, 480, { button: "right" });
  await expect
    .poll(
      async () => {
        const current = (await actors()).find((a) => a.id === initialMonk.id);
        return current
          ? Math.hypot(current.x - initialMonk.x, current.y - initialMonk.y)
          : 0;
      },
      { timeout: 20_000 },
    )
    .toBeGreaterThan(100);
  await expect(page.locator("#unit-state")).toHaveText("unit idle", {
    timeout: 20_000,
  });

  // Ordinary combat wounds the Knight, then an idle Monk heals it. Capture
  // authoritative HP growth, so a nearby healthy unit cannot stand in as proof.
  const enemy = (await actors()).find((a) => a.kind === "Militia" && !a.owned);
  if (!enemy) throw new Error("Enemy militia missing for healing proof");
  await select("Knight");
  await page.mouse.click(enemy.x, enemy.y, { button: "right" });
  await expect
    .poll(
      async () => {
        const knight = (await actors()).find((a) => a.kind === "Knight");
        return knight !== undefined && knight.hp < knight.maxHp;
      },
      { timeout: 20_000 },
    )
    .toBe(true);
  const wounded = (await actors()).find((a) => a.kind === "Knight");
  const monk = (await actors()).find((a) => a.kind === "Monk");
  if (!wounded || !monk) throw new Error("Healing actors missing");
  await page.mouse.click(monk.x, monk.y, { button: "right" });
  await expect
    .poll(
      async () => (await actors()).find((a) => a.id === wounded.id)?.hp ?? 0,
      { timeout: 20_000 },
    )
    .toBeGreaterThan(wounded.hp);
  await capture("monk-healing");
  await writeFile(
    "../.cache/ui/castle-healing.json",
    JSON.stringify(
      {
        wounded,
        healed: (await actors()).find((a) => a.id === wounded.id),
        monk,
      },
      null,
      2,
    ),
  );
}
