import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { fiveVillagerFixture } from "./fixture.js";
import { freshHud } from "./fixtures/resources.js";

for (const button of ["middle", "right"] as const) {
  test(`${button} release ends a placement pan without a build order`, async ({
    page,
  }) => {
    await gameAssets(page, true);
    const orders: number[] = [];
    await fiveVillagerFixture(page, (bytes) => {
      if (bytes[0] !== undefined && bytes[0] > 1) orders.push(bytes[0]);
    });
    await page.goto("/?play=1#x=8192&y=8192&zoom=1");
    await expect(page.locator("#population")).toHaveText("6/10");
    const actors = JSON.parse(
      (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
    ) as { kind: string; x: number; y: number }[];
    const worker = actors.find((actor) => actor.kind === "Villager");
    if (!worker) throw new Error("Fixture villager missing");
    await page.mouse.click(worker.x, worker.y);
    await page.locator("#build-menu").click();
    await page.locator("#build-house").click();
    await page.mouse.move(1000, 240);
    await page.mouse.down({ button: "middle" });
    await page.mouse.move(1040, 260, { steps: 4 });
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-placement-valid",
      "true",
    );
    if (button === "middle") await page.mouse.up({ button });
    else {
      // A secondary release while the middle pan owns capture must also clear it.
      await page.locator("#scene").dispatchEvent("pointerup", {
        button: 2,
        pointerId: 1,
        clientX: 1040,
        clientY: 260,
      });
    }
    await freshHud(page);
    expect(orders).toEqual([]);
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-order-mode",
      "build",
    );
    const camera = () =>
      page
        .locator("#minimap")
        .evaluate((node) => [
          node.getAttribute("data-camera-x"),
          node.getAttribute("data-camera-y"),
        ]);
    const settled = await camera();
    await page.mouse.move(1080, 280);
    await freshHud(page);
    expect(await camera()).toEqual(settled);
    if (button === "right") await page.mouse.up({ button: "middle" });
  });
}
