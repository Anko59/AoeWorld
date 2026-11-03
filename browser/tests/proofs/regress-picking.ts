import { expect, type Page } from "@playwright/test";
import {
  building,
  inputFixture,
  screen,
  selectWorker,
} from "../village/input/fixture.js";

export async function regressPickingProof(page: Page, url: string) {
  const commands = await inputFixture(
    page,
    "webgpu",
    [building(20, 0, 8190, 8190, 4)],
    false,
    0,
    undefined,
    {
      originalArt: true,
      url,
      resources: [
        { kind: 0, name: "berry", gather: "forage", x: 8190, y: 8191 },
      ],
    },
  );
  await page.mouse.click(710, 270);
  await expect(page.locator("#selection-name")).toHaveText("Town Center");
  await page.mouse.click(640, 360);
  await expect(page.locator("#selection")).toHaveAttribute(
    "data-kind",
    "Scout",
  );
  const berry = screen(8190.5, 8191.5);
  await page.mouse.click(berry.x, berry.y);
  await expect(page.locator("#selection")).toHaveAttribute(
    "data-kind",
    "Resource",
  );
  await page.screenshot({ path: "../.cache/showcase/berries-behind-tc.png" });
  await selectWorker(page);
  await page.mouse.move(berry.x, berry.y);
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-command-cursor",
    "gather",
  );
  await page.mouse.click(berry.x, berry.y, { button: "right" });
  await expect.poll(() => commands.at(-1)?.[0]).toBe(4);
  await page.screenshot({
    path: "../.cache/showcase/berries-behind-tc-order.png",
  });
}
