import { expect } from "@playwright/test";
import { PNG } from "pngjs";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { activateSyntheticMap } from "../synthetic-map.js";
import { freshHud } from "../village/fixtures/resources.js";
import { forceCanvas, forceWebGl } from "./backends.js";
import { colours, ownersFixture, playerMask } from "./fixtures/owners.js";

const actors = async (page: import("@playwright/test").Page) =>
  JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as { id: number; x: number; y: number }[];

test("each owner colours its mask and minimap; foreign selections have no commands or orders", async ({
  page,
}, info) => {
  test.setTimeout(120_000);
  if (info.project.name === "canvas") await forceCanvas(page);
  else if (info.project.name !== "webgpu") await forceWebGl(page);
  // Complete native idle villager rows exercise real player masks, not markers.
  await gameAssets(page, true, [[1479, 75]]);
  await playerMask(page);
  const orders = await ownersFixture(page);
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.goto("/?play=1#x=8192.5&y=8192.5&zoom=1");
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    info.project.name === "webgpu"
      ? "webgpu"
      : info.project.name === "canvas"
        ? "canvas2d"
        : "webgl2",
  );
  // Flat ground keeps every owner probe independent of the relief fixture.
  await activateSyntheticMap(page, () => false);
  await freshHud(page);
  const image = PNG.sync.read(await page.locator("#scene").screenshot());
  const all = await actors(page);
  for (let id = 0; id < 8; id++) {
    const actor = all.find((a) => a.id === id);
    if (!actor) throw new Error(`Missing actor ${id}`);
    const at =
      (Math.round(actor.y - 20) * image.width + Math.round(actor.x - 5)) * 4;
    expect(
      Array.from(image.data.subarray(at, at + 3)),
      `player ${id + 1} body`,
    ).toEqual(colours[id]);
    const marker = page
      .locator(`#minimap-entities [data-player="${id}"]`)
      .first();
    await expect(marker).toHaveAttribute(
      "fill",
      `rgb(${colours[id]?.join(",")})`,
    );
  }
  for (const [id, colour] of [
    [8, [59, 132, 223]],
    [9, [249, 235, 130]],
  ] as const) {
    const extra = all.find((a) => a.id === id);
    if (!extra) throw new Error(`Missing extended-palette actor ${id}`);
    const at =
      (Math.round(extra.y - 20) * image.width + Math.round(extra.x - 5)) * 4;
    expect(
      Array.from(image.data.subarray(at, at + 3)),
      `extended player ${id}`,
    ).toEqual(colour);
  }
  // A blue texel outside the source player mask must stay native blue, even on red's sprite.
  const red = all.find((a) => a.id === 1);
  if (!red) throw new Error("Missing foreign villager");
  const sentinel =
    (Math.round(red.y - 20) * image.width + Math.round(red.x + 15)) * 4;
  expect(Array.from(image.data.subarray(sentinel, sentinel + 3))).toEqual(
    colours[0],
  );
  for (const [id, owner] of [
    [1, "Player 2"],
    [40, "Player 4"],
  ] as const) {
    const actor = (await actors(page)).find((a) => a.id === id);
    if (!actor) throw new Error(`Missing foreign actor ${id}`);
    await page.mouse.click(actor.x, actor.y - 10);
    await expect(page.locator("#selection-owner")).toHaveText(owner);
    await freshHud(page);
    const selection = PNG.sync.read(await page.locator("#scene").screenshot());
    let redRing = 0;
    // The Town Center's 4x4 footprint has a 128px vertical ring radius.
    const ringY = actor.y + (id === 40 ? 128 : 0);
    for (let y = Math.floor(ringY - 3); y < ringY + 4; y++)
      for (let x = Math.floor(actor.x - 35); x < actor.x + 36; x++) {
        const at = (y * selection.width + x) * 4;
        if (
          selection.data[at] === 245 &&
          selection.data[at + 1] === 65 &&
          selection.data[at + 2] === 65
        )
          redRing++;
      }
    expect(
      redRing,
      "foreign selection uses the hostile red cue",
    ).toBeGreaterThan(10);
    await expect(page.locator("#command-grid button:visible")).toHaveCount(0);
    if (id === 40) {
      // The S hotkey can pan the camera; use this selection's current frame.
      const at =
        (Math.round(actor.y - 20) * selection.width + Math.round(actor.x - 5)) *
        4;
      expect(
        Array.from(selection.data.subarray(at, at + 3)),
        "building owner colour",
      ).toEqual(colours[3]);
      // Hidden controls must also retain client-side ownership guards.
      for (const command of [
        "train-archer",
        "research-loom",
        "cancel-construction",
        "cancel-research",
        "market-buy-wood",
      ])
        await page.locator(`#${command}`).dispatchEvent("click");
    }
    await page.keyboard.press("q");
    await page.keyboard.press("s");
    await page.mouse.click(1100, 600, { button: "right" });
    await freshHud(page);
    expect(orders).toEqual([]);
  }
});
