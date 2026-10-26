import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { inputFixture } from "../input/fixture.js";

test("comma and the military button cycle owned idle soldiers independently of villagers", async ({
  page,
}, info) => {
  await inputFixture(
    page,
    info.project.name === "webgpu" ? "webgpu" : "webgl2",
    [],
    false,
    0,
    undefined,
    { military: true },
  );
  await page.mouse.move(640, 360);
  await expect(page.locator("#idle-military-count")).toHaveText("2");
  await expect(page.locator("#idle-villager-count")).toHaveText("1");
  const selected = async () =>
    page.locator("#scene").evaluate((canvas) =>
      (
        JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as {
          id: number;
          selected: boolean;
        }[]
      )
        .filter((actor) => actor.selected)
        .map((actor) => actor.id),
    );
  for (const [id, x, y, button] of [
    [0, 8192.5, 8192.5, false],
    [3, 8196.5, 8194.5, true],
    [0, 8192.5, 8192.5, false],
  ] as const) {
    if (button) await page.locator("#idle-military").click();
    else await page.keyboard.press(",");
    await expect.poll(selected).toEqual([id]);
    for (const [axis, value] of [
      ["x", x],
      ["y", y],
    ] as const)
      await expect
        .poll(async () =>
          Number(
            await page.locator("#minimap").getAttribute(`data-camera-${axis}`),
          ),
        )
        .toBeCloseTo(value, 1);
  }
  await page.keyboard.press(".");
  await expect.poll(selected).toEqual([2]);
  await page.keyboard.press(",");
  await expect.poll(selected).toEqual([3]);
  await page.keyboard.press("Control+,");
  await expect.poll(selected).toEqual([3]);
  await page.locator("#menu-toggle").click();
  await page.keyboard.press(",");
  await expect.poll(selected).toEqual([3]);
});
