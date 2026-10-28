import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { inputFixture, screen, selectWorker } from "./fixture.js";

test("all ten control groups assign, select, extend and double-tap center", async ({
  page,
}, info) => {
  test.setTimeout(90_000);
  const commands = await inputFixture(
    page,
    info.project.name === "webgpu" ? "webgpu" : "webgl2",
  );
  const canvas = page.locator("#scene");
  for (const number of ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"]) {
    await selectWorker(page);
    await page.keyboard.press(`Control+${number}`);
    await page.keyboard.press("Escape");
    await page.keyboard.press(number);
    await expect(page.locator("#selection-name")).toHaveText("Villager");
    await expect(page.locator("#selection")).toHaveAttribute("data-count", "1");
  }
  await selectWorker(page, 3);
  await page.keyboard.press("Shift+1");
  await page.keyboard.press("Escape");
  await page.keyboard.press("1");
  await expect(page.locator("#selection-name")).toHaveText(
    "2 Villagers selected",
  );
  const before = commands.length;
  await page.keyboard.down("Control");
  await page.mouse.click(1000, 400, { button: "right" });
  await page.keyboard.up("Control");
  await expect.poll(() => commands.length).toBe(before + 2);
  expect(commands.slice(before).map((c) => c[0])).toEqual([21, 21]);
  expect(commands.slice(before).map((c) => c[2])).toEqual([2, 3]);
  // Single recall preserves the current camera; a double tap centers the group's centroid.
  await page.evaluate(() => {
    location.hash = "x=8202&y=8202&zoom=.6";
  });
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-camera-x",
    "8202",
  );
  await page.waitForTimeout(400);
  await page.keyboard.press("1");
  await page.evaluate(
    () =>
      new Promise<void>((resolve) => requestAnimationFrame(() => resolve())),
  );
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-camera-x",
    "8202",
  );
  // A double tap is two presses within 350 ms; the frame wait and assertion
  // above can exceed that under software WebGPU, so tap twice afresh.
  await page.waitForTimeout(400);
  await page.keyboard.press("1");
  await page.keyboard.press("1");
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-camera-x",
    "8195.5",
  );
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-camera-y",
    "8194.5",
  );
  // A foreign unit must never enter a group, even when explicitly selected.
  await page.keyboard.press("Home");
  const foreign = screen(8189.5, 8192.5);
  await page.mouse.click(foreign.x, foreign.y);
  await page.keyboard.press("Control+2");
  await page.keyboard.press("Escape");
  await page.keyboard.press("2");
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "0");
  // An absent subscription member is retained; a known dead/disowned member is removed.
  const retained = await page.evaluate(() => {
    const navigation = Reflect.get(window, "aoeNavigation");
    const absent = navigation.group(11, [], new Float64Array());
    navigation.groupsUpdate(new Float64Array([2, NaN, NaN, 3, NaN, NaN]));
    return [absent, navigation.group(11, [], new Float64Array())];
  });
  expect(retained).toEqual([[2, 3], []]);
  await expect(canvas).toBeVisible();
});
