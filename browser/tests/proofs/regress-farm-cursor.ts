import { expect, type Page } from "@playwright/test";
import {
  building,
  inputFixture,
  screen,
  selectWorker,
} from "../village/input/fixture.js";

export async function regressFarmCursorProof(page: Page, url: string) {
  const commands = await inputFixture(
    page,
    "webgpu",
    [building(30, 5, 8197, 8188, 3, 900, true, 0, 175)],
    false,
    0,
    undefined,
    { originalArt: true, url },
  );
  await selectWorker(page);
  const farm = screen(8198.5, 8189.5);
  await page.mouse.move(farm.x, farm.y);
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-command-cursor",
    "move",
  );
  await page.screenshot({
    path: "../.cache/showcase/farm-cursor-original.png",
  });
  await page.mouse.click(farm.x, farm.y, { button: "right" });
  await expect.poll(() => commands.at(-1)?.[0]).toBe(11);
}
