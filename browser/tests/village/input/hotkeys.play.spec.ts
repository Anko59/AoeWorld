import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { inputFixture, building, selectWorker } from "./fixture.js";

test("economic and military grid keys never pan the camera, arrows still pan", async ({
  page,
}, info) => {
  test.setTimeout(90_000);
  const renderer = info.project.name === "webgpu" ? "webgpu" : "webgl2";
  await inputFixture(page, renderer, [building(20, 4, 8198, 8188, 2)]);
  await selectWorker(page);
  const minimap = page.locator("#minimap");
  const center = await minimap.getAttribute("data-camera-x");
  await page.keyboard.press("b");
  await expect(page.locator("#build-farm")).toBeEnabled();
  await page.keyboard.down("a");
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-order-mode",
    "build",
  );
  await page.waitForTimeout(300);
  await page.keyboard.up("a");
  await expect(minimap).toHaveAttribute("data-camera-x", center ?? "");
  await page.keyboard.press("Escape");
  // Every alphabetic camera alias conflicted with an economic/military command.
  for (const key of ["a", "w", "s", "d"]) {
    await page.keyboard.down(key);
    await page.waitForTimeout(100);
    await page.keyboard.up(key);
    await expect(minimap).toHaveAttribute("data-camera-x", center ?? "");
  }
  await page.keyboard.press("ArrowRight");
  await expect(minimap).not.toHaveAttribute("data-camera-x", center ?? "");
});
