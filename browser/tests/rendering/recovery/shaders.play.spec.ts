import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { bluePixels } from "../play-pixels.js";

test("WebGPU loads both generated shader assets before drawing", async ({
  page,
  request,
}) => {
  test.setTimeout(90_000);
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page, true);
  const sources: string[] = [];
  await page.route("**/pkg/*.wgsl", async (route) => {
    const response = await route.fetch();
    expect(response.ok()).toBe(true);
    const source = await response.text();
    expect(source).toContain("fn vs_main(");
    expect(source).toContain("fn fs_main(");
    expect(source).toContain("fn fs_unit_contour(");
    sources.push(new URL(route.request().url()).pathname);
    await route.fulfill({ response });
  });
  await page.goto("/?play=1");
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    "webgpu",
    { timeout: 60_000 },
  );
  await expect
    .poll(() => bluePixels(page.locator("#scene")))
    .toBeGreaterThan(10);
  expect(sources.sort()).toEqual(["/pkg/mixing.wgsl", "/pkg/ordinary.wgsl"]);
});

for (const failure of ["missing", "invalid"] as const) {
  test(`WebGPU ${failure} shader asset falls back and draws`, async ({
    page,
    request,
  }) => {
    test.setTimeout(90_000);
    expect((await request.post("/maps/reset")).status()).toBe(204);
    await gameAssets(page, true);
    await page.route("**/pkg/mixing.wgsl", (route) =>
      route.fulfill({
        status: failure === "missing" ? 404 : 200,
        body: "invalid shader asset",
      }),
    );
    await page.goto("/?play=1");
    await expect(page.locator("#playground")).toHaveAttribute(
      "data-renderer",
      "webgl2",
      { timeout: 60_000 },
    );
    await expect(page.locator("#connection")).toHaveText("connected");
    await expect
      .poll(() => bluePixels(page.locator("#scene")))
      .toBeGreaterThan(10);
  });
}
