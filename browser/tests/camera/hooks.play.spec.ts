import { expect, test, type Page } from "@playwright/test";
import { PNG } from "pngjs";
import { gameAssets } from "../game-assets.js";

async function camera(page: Page, x: string, y: string, zoom: string) {
  await expect(page.locator("#minimap")).toHaveAttribute("data-camera-x", x);
  await expect(page.locator("#minimap")).toHaveAttribute("data-camera-y", y);
  await expect(page.locator("#minimap")).toHaveAttribute("data-zoom", zoom);
}

test("camera fragment parser preserves decimals, invalid parts, and zoom limits", async ({
  page,
}) => {
  await page.goto("/diagnostics.html");
  const parsed = await page.evaluate(async () => {
    const module = (await import("/navigation.js" as string)) as {
      parseCameraFragment(hash: string): Float64Array;
    };
    const parse = (hash: string) =>
      Array.from(module.parseCameraFragment(hash), (value) =>
        Number.isNaN(value) ? null : value,
      );
    return {
      decimal: parse("#x=300.25&y=400.5&zoom=1.75&renderer=canvas"),
      invalid: ["", "nope", "NaN", "inf", "1e999", " ", "0x10", "1\n"].map(
        (value) => [
          parse(`#x=${value}&y=400.5&zoom=2`),
          parse(`#x=300.25&y=${value}&zoom=${value}`),
        ],
      ),
      zoomOnly: parse("#zoom=2"),
      high: parse("#x=-500&y=9999999&zoom=99"),
      low: parse("#zoom=-5"),
      unrelated: parse("#unrelated=value"),
      renderers: ["invalid", "webgpu", "webgl2", "canvas"].map((value) =>
        parse(`#renderer=${value}`),
      ),
      repeated: parse(
        "#x=10&x=20&zoom=2&zoom=bad&renderer=canvas&renderer=bad",
      ),
      syntax: parse("##x=+3.25&y=.5e2&zoom=1."),
      literal: parse("#x=%31&y=1+2&zoom=1%2E5"),
      unknown: parse("#toString=1&__proto__=2&renderer=constructor"),
      bareKeys: parse("#x=10&x&zoom=2&zoom&renderer=canvas&renderer"),
      question: parse("#?x=7&y=2"),
      hostile: (() => {
        const started = performance.now();
        const value = parse("#x=" + "1".repeat(80000) + "!&y=290.5&zoom=2");
        return { value, elapsed: performance.now() - started };
      })(),
    };
  });
  expect(parsed.decimal).toEqual([300.25, 400.5, 1.75, 3]);
  for (const [invalidX, invalidYZoom] of parsed.invalid) {
    expect(invalidX).toEqual([null, 400.5, 2, 0]);
    expect(invalidYZoom).toEqual([300.25, null, null, 0]);
  }
  expect(parsed.zoomOnly).toEqual([null, null, 2, 0]);
  expect(parsed.high).toEqual([-500, 9999999, 3, 0]);
  expect(parsed.low).toEqual([null, null, 0.25, 0]);
  expect(parsed.unrelated).toEqual([null, null, null, 0]);
  expect(parsed.renderers).toEqual([
    [null, null, null, 0],
    [null, null, null, 1],
    [null, null, null, 2],
    [null, null, null, 3],
  ]);
  expect(parsed.repeated).toEqual([20, null, null, 0]);
  expect(parsed.syntax).toEqual([3.25, 50, 1, 0]);
  expect(parsed.literal).toEqual([null, null, null, 0]);
  expect(parsed.unknown).toEqual([null, null, null, 0]);
  expect(parsed.bareKeys).toEqual([10, null, 2, 3]);
  expect(parsed.question).toEqual([null, 2, null, 0]);
  // A crafted link must be rejected promptly, not by exploring digit splits.
  expect(parsed.hostile.value).toEqual([null, 290.5, 2, 0]);
  expect(parsed.hostile.elapsed).toBeLessThan(250);
});

test("spectator camera fragments settle on controller-opened terrain and pan without history writes", async ({
  page,
  browser,
  request,
}) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page, true);
  await page.goto("/?play=1");
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
  await page.getByRole("button", { name: "Menu", exact: true }).click();
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
