import { expect } from "@playwright/test";
import { PNG } from "pngjs";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { workerFixture } from "../village/fixtures/worker.js";
import { freshHud } from "../village/fixtures/resources.js";
import { forceWebGl } from "./backends.js";
import { chunks, nodes } from "./fixtures/hover.js";

for (const mode of ["cursor", "outline", "glow"] as const) {
  test(`resource hover comparison: ${mode}`, async ({ page }, info) => {
    test.setTimeout(120_000);
    if (info.project.name !== "webgpu") await forceWebGl(page);
    // gameAssets already supplies the current berry/gold rows. Repeating
    // them would create duplicate source frames, which startup rejects.
    await gameAssets(page, true);
    await workerFixture(page);
    await page.setViewportSize({ width: 1280, height: 720 });
    await page.goto(`/?play=1&hover=${mode}#x=8192&y=8192&zoom=.8`);
    await expect(page.locator("#connection")).toHaveText("connected", {
      timeout: 60_000,
    });
    await expect(page.locator("#playground")).toHaveAttribute(
      "data-renderer",
      info.project.name === "webgpu" ? "webgpu" : "webgl2",
    );
    await page.evaluate(async (chunks) => {
      const module = await import(
        new URL("/pkg/aoe_client.js", location.href).href
      );
      module.activate_surface_fixture("04".repeat(32), chunks);
    }, JSON.stringify(chunks()));
    // Flat fixture replaces source heights; restore the matching flat focus.
    await page.keyboard.press("Home");
    await freshHud(page);
    const actors = JSON.parse(
      (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
    ) as { id: number; x: number; y: number }[];
    const worker = actors.find((actor) => actor.id === 3);
    if (!worker) throw new Error("Frozen villager missing");
    await page.mouse.click(worker.x, worker.y);
    await expect(page.locator("#selection-name")).toHaveText("Villager");
    for (const node of nodes) {
      // Camera stays at the frozen scout (8192.5,8192.5), flat focus, zoom .8.
      const x = 640 + (node.x - node.y) * 64 * 0.8;
      const y = 360 + (node.x + node.y - 16384) * 32 * 0.8;
      await page.mouse.move(1100, 180);
      await freshHud(page);
      const clip = {
        x: Math.floor(x - 80),
        y: Math.floor(y - 180),
        width: 160,
        height: 210,
      };
      const before = PNG.sync.read(await page.screenshot({ clip }));
      await page.mouse.move(x, y);
      await expect(page.locator("#scene")).toHaveAttribute(
        "data-gather-kind",
        node.gather,
      );
      await expect(page.locator("#scene")).toHaveAttribute(
        "data-resource-hovered",
        "true",
      );
      await expect(page.locator("#scene")).toHaveAttribute(
        "data-hover-mode",
        mode,
      );
      await expect(page.locator("#resource-highlight")).toBeHidden();
      await freshHud(page);
      const after = PNG.sync.read(await page.screenshot({ clip }));
      let changed = 0;
      for (let at = 0; at < before.data.length; at += 4)
        if (
          [0, 1, 2].some(
            (channel) =>
              Math.abs(
                (before.data[at + channel] ?? 0) -
                  (after.data[at + channel] ?? 0),
              ) > 10,
          )
        )
          changed++;
      if (mode === "cursor") expect(changed).toBe(0);
      else expect(changed).toBeGreaterThan(20);
    }
    await page.mouse.move(1100, 180);
    await freshHud(page);
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-resource-hovered",
      "false",
    );
  });
}
