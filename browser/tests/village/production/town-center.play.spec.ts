import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { selectWorker } from "../input/fixture.js";
import { playerState, productionFixture } from "./fixture.js";

test.setTimeout(90_000);

test("villagers see additional Town Centers from Castle Age", async ({
  page,
}) => {
  const { update } = await productionFixture(page, []);
  await selectWorker(page);
  await page.locator("#build-menu").click();
  const build = page.locator("#build-town-center");
  await expect(build).toBeVisible();
  await expect(build).toBeDisabled();
  update(playerState(1, 2 ** 2));
  await expect(build).toBeDisabled();
  update(playerState(2, 2 ** 2 + 2 ** 14));
  await expect(build).toBeEnabled();
  await build.click();
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-order-mode",
    "build",
  );
});
