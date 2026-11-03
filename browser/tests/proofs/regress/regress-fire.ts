import { expect, type Page } from "@playwright/test";
import { building, inputFixture } from "../../village/input/fixture.js";

/** Frozen health, with the real client and locally served original art. */
export async function regressFireProof(page: Page, url: string) {
  await inputFixture(
    page,
    "webgpu",
    [building(20, 0, 8190, 8190, 4, 350)],
    false,
    0,
    undefined,
    { originalArt: true, url },
  );
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-village-art",
    "native",
  );
  await expect(page.locator("html")).toHaveAttribute(
    "data-interface-art",
    "original",
  );
  await page.mouse.move(600, 330);
  await page.mouse.down();
  await page.mouse.move(760, 500, { steps: 3 });
  await page.mouse.up();
  await expect(page.locator("#selected-icons .unit-token")).toHaveCount(3);
  await page
    .locator('#selected-icons [data-unit-id="2"]')
    .click({ modifiers: ["Control"] });
  await expect(page.locator("#selected-icons .unit-token")).toHaveCount(2);
  await page.screenshot({ path: "../.cache/showcase/portraits-original.png" });
  await page.keyboard.press("h");
  await expect(page.locator("#selection-name")).toHaveText("Town Center");
  await expect(page.locator("#research-feudal-age")).toBeDisabled();
  await expect(page.locator("#research-feudal-age")).toHaveCSS(
    "filter",
    "none",
  );
  await page.screenshot({
    path: "../.cache/showcase/command-colours-original.png",
  });
  await page.keyboard.press("Escape");
  await page.mouse.move(640, 480);
  await page.waitForTimeout(2000);
  await page.screenshot({ path: "../.cache/showcase/burning-building.png" });
  await page.waitForTimeout(3000);
  const video = page.video();
  await page.close();
  await video?.saveAs("../.cache/showcase/burning-building-raw.webm");
}
