import { expect } from "@playwright/test";
import { PNG } from "pngjs";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { centerVillage, freshHud } from "../fixtures/resources.js";

for (const original of [false, true]) {
  test(`selected rally flag has rendered pixels, original art=${original}`, async ({
    page,
    request,
  }, info) => {
    test.setTimeout(90_000);
    expect((await request.post("/maps/reset")).status()).toBe(204);
    await gameAssets(page, true, [], original ? [] : [4480]);
    const renderer = info.project.name === "webgpu" ? "webgpu" : "webgl2";
    await page.goto(`/?play=1#zoom=.5&renderer=${renderer}`);
    await expect(page.locator("#connection")).toHaveText("connected", {
      timeout: 60_000,
    });
    await expect(page.locator("#playground")).toHaveAttribute(
      "data-renderer",
      renderer,
    );
    await centerVillage(page, true);
    const canvas = page.locator("#scene");
    const center = (
      JSON.parse((await canvas.getAttribute("data-actors")) ?? "[]") as {
        kind: string;
        x: number;
        y: number;
      }[]
    ).find((a) => a.kind === "TownCenter");
    if (!center) throw new Error("Town Center missing");
    const target = { x: center.x, y: center.y + 110 };
    await page.mouse.click(target.x, target.y, { button: "right" });
    await expect(canvas).toHaveAttribute("data-rally-point", /.+/);
    await page.mouse.move(1000, 200);
    await freshHud(page);
    const clip = {
      x: Math.floor(target.x - 20),
      y: Math.floor(target.y - 45),
      width: 45,
      height: 48,
    };
    const selected = PNG.sync.read(await page.screenshot({ clip }));
    await page.keyboard.press("Escape");
    await expect(canvas).toHaveAttribute("data-rally-point", "");
    await freshHud(page);
    const cleared = PNG.sync.read(await page.screenshot({ clip }));
    let changed = 0;
    for (let at = 0; at < selected.data.length; at += 4) {
      if (
        [0, 1, 2].some(
          (c) =>
            Math.abs(
              (selected.data[at + c] ?? 0) - (cleared.data[at + c] ?? 0),
            ) > 20,
        )
      )
        changed++;
    }
    expect(
      changed,
      "selecting the building must draw the flag at the authoritative rally point",
    ).toBeGreaterThan(20);
    await centerVillage(page, true);
    await expect(canvas).toHaveAttribute("data-rally-point", /.+/);
  });
}
