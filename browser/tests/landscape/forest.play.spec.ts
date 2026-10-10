import { expect, test, type Page } from "@playwright/test";
import { PNG } from "pngjs";
import { gameAssets } from "../game-assets.js";
import { forceCanvas, forceWebGl } from "../rendering/backends.js";

async function primaryBluePosition(page: Page) {
  const image = PNG.sync.read(await page.locator("#scene").screenshot());
  let count = 0;
  let xTotal = 0;
  let yTotal = 0;
  for (
    let y = Math.floor(image.height / 2 - 140);
    y < image.height / 2 + 140;
    y++
  ) {
    for (
      let x = Math.floor(image.width / 2 - 140);
      x < image.width / 2 + 140;
      x++
    ) {
      const index = (y * image.width + x) * 4;
      const [r = 0, g = 0, b = 0] = image.data.subarray(index, index + 3);
      if (b > 150 && b > r + 30 && b > g + 30) {
        count++;
        xTotal += x;
        yTotal += y;
      }
    }
  }
  return {
    count,
    x: xTotal / Math.max(1, count),
    y: yTotal / Math.max(1, count),
  };
}

for (const backend of ["preferred", "webgl2", "canvas"] as const) {
  test(`synthetic prepared forest serves metadata, draws and moves on ${backend}`, async ({
    page,
    request,
  }) => {
    expect((await request.post("/maps/reset")).status()).toBe(204);
    if (backend === "webgl2") await forceWebGl(page);
    if (backend === "canvas") await forceCanvas(page);
    await gameAssets(page);
    const errors: string[] = [];
    const fetched = new Set<string>();
    page.on("pageerror", (error) => errors.push(error.message));
    page.on("response", async (response) => {
      if (!response.url().includes("/chunks/") || !response.ok()) return;
      const body = await response.json();
      if (body.payload_hex?.startsWith("04") && fetched.size < 64)
        fetched.add(response.url());
    });
    await page.goto("/");
    await expect(page.locator("#connection")).toHaveText("connected", {
      timeout: 30_000,
    });
    await expect(page.locator("#playground")).toHaveAttribute(
      "data-assets",
      "aoe2-local",
    );
    const token = await page.evaluate(() =>
      sessionStorage.getItem("aoeworld.resume-token"),
    );
    if (!token) throw new Error("Connected controller token absent");
    expect(token).toMatch(/^[0-9a-f]{48}$/i);
    // Supplied only by the disposable canonical E2E stack. This is an immutable
    // synthetic prepared package, not a claim that fallback Paris is playable.
    const hash = process.env["AOE_E2E_LANDSCAPE_HASH"];
    if (!hash)
      throw new Error(
        "Run through make test-e2e to prepare the synthetic forest",
      );
    expect(hash).toMatch(/^[0-9a-f]{64}$/);
    const response = await request.post(`/maps/${hash}`, {
      headers: { "X-AoeWorld-Controller-Token": token },
    });
    expect(response.ok(), await response.text()).toBeTruthy();
    const activation = await response.json();
    expect(activation.start_available).toBeTruthy();
    expect(activation.content_hash).toBe(hash);
    const packageResponse = await request.get(`/maps/${hash}`);
    expect(packageResponse.ok()).toBeTruthy();
    const pkg = await packageResponse.json();
    expect(pkg.schema_version).toBe(1);
    expect(pkg.generation_recipe_version).toBe(1);
    expect(pkg.request).not.toHaveProperty("detail_profile");
    expect(pkg.estimate.tiles_per_side).toBe(512);
    // Prepared-page tile labels may say SourceDerived; these samples are
    // synthetic test inputs, not observed real geodata or source qualification.
    expect(pkg.source_locks).toEqual([]);
    expect(pkg.projection.horizontal_crs).toBe(
      "synthetic-test-only-constant-grid-v1",
    );
    expect(pkg.provenance).toEqual({
      elevation: "fallback",
      water: "fallback",
      vegetation: "fallback",
      historical_land_use: "fallback",
    });
    expect(pkg.environment.samples_per_axis).toBe(1);
    expect(pkg.environment.vegetation.levels).toHaveLength(1);
    const chunkResponse = await request.get(`/maps/${hash}/chunks/8/8`);
    expect(chunkResponse.ok()).toBeTruthy();
    expect((await chunkResponse.json()).payload_hex).toMatch(/^04/);
    // Activation retires the old world; the existing UI explicitly requires a
    // reconnect to join its replacement. Wait until reset cleared the old token.
    await expect(page.locator("#connection")).toHaveText(/reconnect/);
    await page.getByRole("button", { name: "Reconnect to server" }).click();
    await expect(page.locator("#connection")).toHaveText("connected");
    await expect
      .poll(() => [...fetched].some((url) => url.includes(hash)), {
        timeout: 30_000,
      })
      .toBeTruthy();
    if (backend !== "preferred")
      await expect(page.locator("#playground")).toHaveAttribute(
        "data-renderer",
        backend === "canvas" ? "canvas2d" : backend,
      );
    await page.getByRole("button", { name: "Center on primary unit" }).click();
    await expect
      .poll(async () => (await primaryBluePosition(page)).count, {
        timeout: 30_000,
      })
      .toBeGreaterThan(10);
    const canvas = page.locator("#scene");
    const box = await canvas.boundingBox();
    if (!box) throw new Error("Missing composed map canvas");
    const center = { x: box.width / 2, y: box.height / 2 };
    await canvas.hover({ position: center });
    const inspection = page.locator("#tile-inspection");
    await expect(inspection).toContainText("tile ");
    const tileAtCenter = async () =>
      (await inspection.textContent())?.match(/tile (-?\d+), (-?\d+)/)?.[0];
    const startTile = await tileAtCenter();
    expect(startTile).toBeDefined();
    await canvas.click({ position: center });
    // Cross a real tile boundary at the ordinary zoom while staying within the
    // certified clear footprint. A 35px shift is still inside the starting tile.
    const destination = { x: center.x + 128, y: center.y };
    await canvas.hover({ position: destination });
    await expect(inspection).toContainText("passable");
    expect(await tileAtCenter()).not.toBe(startTile);
    await canvas.click({ position: destination, button: "right" });
    // Recenter from authoritative unit state: sprite animation alone cannot
    // satisfy this assertion by moving blue pixels within a stationary tile.
    await expect
      .poll(
        async () => {
          await page
            .getByRole("button", { name: "Center on primary unit" })
            .click();
          await canvas.hover({ position: { x: center.x + 1, y: center.y } });
          await canvas.hover({ position: center });
          return (await tileAtCenter()) ?? startTile;
        },
        { timeout: 30_000 },
      )
      .not.toBe(startTile);
    expect(errors).toEqual([]);
  });
}
