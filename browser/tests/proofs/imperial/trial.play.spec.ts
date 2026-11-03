import { expect } from "@playwright/test";
import { PNG } from "pngjs";
import { test } from "../../../server.js";
import { centerVillage, freshHud } from "../../village/fixtures/resources.js";

// Private visual proof: the base trial pack is required, and every optional
// overlay request is refused. Public CI keeps the synthetic browser unit proof.
test.use({ video: { mode: "on", size: { width: 1280, height: 720 } } });
test("trial buildings remain visible through Imperial research without DE", async ({
  page,
  request,
}, info) => {
  test.setTimeout(150_000);
  const manifest = await request.get("/asset-pack/manifest.json");
  test.skip(!manifest.ok(), "Requires a privately imported trial pack");
  await page.route("**/asset-pack/buildings/**", (route) =>
    route.fulfill({ status: 404 }),
  );
  expect(
    (await request.post("/maps/reset?scenario=imperial-army")).status(),
  ).toBe(204);
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.goto("/?play=1#x=98&y=134&zoom=.5");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  const scene = page.locator("#scene");
  await expect(scene).toHaveAttribute("data-village-art", "native");
  await centerVillage(page, true);
  await expect(page.locator("#age")).toContainText("Castle Age");
  const castle = PNG.sync.read(
    await page.screenshot({ path: info.outputPath("castle-trial.png") }),
  );
  await expect(page.locator("#research-imperial-age")).toBeEnabled();
  await page.locator("#research-imperial-age").click();
  await expect(page.locator("#age")).toContainText("Imperial Age", {
    timeout: 30_000,
  });
  await freshHud(page);
  const imperial = PNG.sync.read(
    await page.screenshot({ path: info.outputPath("imperial-trial.png") }),
  );
  // The centered TC roof sits above a generic marker's extent. Preserve its
  // pixels through age-up; allow small differences from moving unit overlays.
  let changed = 0;
  for (let y = 235; y < 285; y++) {
    for (let x = 590; x < 690; x++) {
      const offset = (y * castle.width + x) * 4;
      if (
        [0, 1, 2].some(
          (channel) =>
            Math.abs(
              castle.data.readUInt8(offset + channel) -
                imperial.data.readUInt8(offset + channel),
            ) > 16,
        )
      )
        changed++;
    }
  }
  expect(changed / 5000).toBeLessThan(0.1);
  await expect(scene).toHaveAttribute("data-village-art", "native");
  await expect(page.locator("#selection-name")).toHaveText("Town Center");
  await page.waitForTimeout(1500); // Hold the completed age-up in the proof video.
});
