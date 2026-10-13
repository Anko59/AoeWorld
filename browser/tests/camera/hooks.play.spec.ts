import { expect, test, type Page } from "@playwright/test";
import { PNG } from "pngjs";
import { gameAssets } from "../game-assets.js";

async function camera(page: Page, x: string, y: string, zoom: string) {
  await expect(page.locator("#minimap")).toHaveAttribute("data-camera-x", x);
  await expect(page.locator("#minimap")).toHaveAttribute("data-camera-y", y);
  await expect(page.locator("#minimap")).toHaveAttribute("data-zoom", zoom);
}

test("spectator camera fragments settle on controller-opened terrain and pan without history writes", async ({
  page,
  browser,
  request,
}) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page, true);
  await page.goto("/");
  await expect(page.locator("#connection")).toHaveText("connected");
  const token = await page.evaluate(() =>
    sessionStorage.getItem("aoeworld.resume-token"),
  );
  expect(token).toBeTruthy();
  const hash = process.env["AOE_E2E_LANDSCAPE_HASH"];
  if (!hash)
    throw new Error("The disposable E2E stack supplies a prepared map");
  const activation = await page.evaluate(async (hash) => {
    const response = await fetch(`/maps/${hash}`, {
      method: "POST",
      headers: {
        "x-aoeworld-controller-token":
          sessionStorage.getItem("aoeworld.resume-token") ?? "",
      },
    });
    if (!response.ok) throw new Error(await response.text());
    return response.json() as Promise<{
      content_hash: string;
      start_available: boolean;
    }>;
  }, hash);
  expect(activation.start_available).toBe(true);
  await expect(page.locator("#connection")).toHaveText(/reconnect/);
  await page.getByRole("button", { name: "Reconnect to server" }).click();
  await expect
    .poll(() =>
      page.evaluate(() => sessionStorage.getItem("aoeworld.resume-token")),
    )
    .not.toBe(token);
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect
    .poll(() =>
      page.evaluate(() => sessionStorage.getItem("aoeworld.resume-token")),
    )
    .toBeTruthy();
  const context = await browser.newContext();
  try {
    const spectator = await context.newPage();
    await gameAssets(spectator, true);
    const url = new URL(
      "/#x=256.25&y=256.5&zoom=1.75&renderer=canvas",
      page.url(),
    ).href;
    await spectator.goto(url);
    await expect(spectator.locator("#connection")).toHaveText("connected");
    await expect(spectator.locator("#minimap")).toHaveAttribute(
      "data-map-hash",
      activation.content_hash,
    );
    await expect(spectator.locator("#playground")).toHaveAttribute(
      "data-renderer",
      "canvas2d",
    );
    await camera(spectator, "256.25", "256.5", "1.75");
    await expect(spectator.locator("#scene")).toHaveAttribute(
      "data-chunks-pending",
      "0",
    );
    await expect(spectator.locator("#scene")).toHaveAttribute(
      "data-chunks-failed",
      "0",
    );
    const before = PNG.sync.read(
      await spectator.locator("#scene").screenshot(),
    );
    // Terrain fills the central viewport: neither blank background nor a sprite suffices.
    const colors = new Set<string>();
    let terrainPixels = 0;
    for (let y = 150; y < 450; y += 10)
      for (let x = 250; x < 650; x += 10) {
        const offset = (y * before.width + x) * 4;
        const color = before.data.subarray(offset, offset + 3).toString("hex");
        // Current composed grass base/accent for the flat temperate package,
        // including its geographic appearance tint; exclude the blue horse.
        if (color === "457835" || color === "2a5225") {
          terrainPixels++;
          colors.add(color);
        }
      }
    expect(colors).toEqual(new Set(["457835", "2a5225"]));
    expect(terrainPixels).toBeGreaterThan(1000);
    await expect(spectator.locator("#terrain-cache")).toHaveText(
      /\([1-9]\d* \/ 512 chunks\)/,
    );
    const frame = Number(
      await spectator.locator("#scene").getAttribute("data-frame"),
    );
    await spectator.evaluate(() => {
      location.hash = "x=300.5&y=280.25&zoom=2&renderer=webgpu";
    });
    await camera(spectator, "300.5", "280.25", "2");
    await expect(spectator.locator("#playground")).toHaveAttribute(
      "data-renderer",
      "canvas2d",
    );
    await expect(spectator.locator("#scene")).toHaveAttribute(
      "data-chunks-pending",
      "0",
    );
    await expect
      .poll(async () =>
        Number(await spectator.locator("#scene").getAttribute("data-frame")),
      )
      .toBeGreaterThan(frame);
    await spectator.evaluate(() => {
      location.hash = "x=bad&y=290.5&zoom=NaN";
    });
    await camera(spectator, "300.5", "290.5", "2");
    await spectator.mouse.move(600, 300);
    await spectator.keyboard.press("ArrowRight");
    await expect
      .poll(async () =>
        Number(
          await spectator.locator("#minimap").getAttribute("data-camera-x"),
        ),
      )
      .toBeGreaterThan(300.5);
    expect(new URL(spectator.url()).hash).toBe("#x=bad&y=290.5&zoom=NaN");
    await expect(spectator.locator("#scene")).toHaveAttribute(
      "data-chunks-pending",
      "0",
    );
    // The spectator gets no lease token, yet its map raster comes from the opened package.
    expect(
      await spectator.evaluate(() =>
        sessionStorage.getItem("aoeworld.resume-token"),
      ),
    ).toBeNull();
    expect(
      await spectator.evaluate(
        async (hash) =>
          (await fetch(`/maps/${hash}`, { method: "POST" })).status,
        hash,
      ),
    ).toBe(403);
    await expect(spectator.locator("#minimap-terrain")).toHaveAttribute(
      "href",
      /^data:image/,
    );
  } finally {
    await context.close();
  }
});
