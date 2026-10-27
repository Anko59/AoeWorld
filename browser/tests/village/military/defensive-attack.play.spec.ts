import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { building, inputFixture } from "../input/fixture.js";

test("right-clicking an enemy building commands the selected Castle weapon", async ({
  page,
}, info) => {
  const renderer = info.project.name === "webgpu" ? "webgpu" : "webgl2";
  const commands = await inputFixture(page, renderer, [
    building(10, 16, 8188, 8188, 4),
    building(11, 1, 8198, 8190, 2, 900, true, 1),
  ]);
  const actors = JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as { id: number; x: number; y: number }[];
  const castle = actors.find((a) => a.id === 10);
  const enemy = actors.find((a) => a.id === 11);
  if (!castle || !enemy) throw new Error("Defensive building fixtures missing");
  await page.mouse.click(castle.x, castle.y);
  await expect(page.locator("#selection-name")).toHaveText("Castle");
  await page.mouse.click(enemy.x, enemy.y, { button: "right" });
  await expect
    .poll(() =>
      commands.some(
        (bytes) => bytes[0] === 7 && bytes[2] === 10 && bytes[3] === 11,
      ),
    )
    .toBe(true);
  expect(commands.some((bytes) => bytes[0] === 20)).toBe(false); // SetRallyOrder
});
