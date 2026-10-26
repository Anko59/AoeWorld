import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { freshHud } from "../village/fixtures/resources.js";
import { forceWebGl } from "./backends.js";

test("WebGL2 decodes each source once and starts a connected village", async ({
  page,
  request,
}, info) => {
  test.setTimeout(100_000);
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
  // Exercise both the explicit backend flag and fallback with no WebGPU API.
  if (info.project.name !== "webgpu") await forceWebGl(page);
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.stack ?? error.message));
  const started = Date.now();
  const assets: { path: string; elapsed_ms: number }[] = [];
  page.on("response", (response) => {
    if (response.url().includes("/asset-pack/")) {
      assets.push({
        path: new URL(response.url()).pathname,
        elapsed_ms: Date.now() - started,
      });
    }
  });
  try {
    await page.goto(
      info.project.name === "webgpu" ? "/?play=1#renderer=webgl2" : "/?play=1",
    );
    await expect(page.locator("#connection")).toHaveText("connected", {
      timeout: 60_000,
    });
    await expect(page.locator("#playground")).toHaveAttribute(
      "data-renderer",
      "webgl2",
    );
    await expect
      .poll(async () =>
        (
          JSON.parse(
            (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
          ) as { kind: string }[]
        ).map((actor) => actor.kind),
      )
      .toEqual(expect.arrayContaining(["TownCenter", "Villager", "Scout"]));
    await expect
      .poll(async () =>
        Number(await page.locator("#scene").getAttribute("data-frame")),
      )
      .toBeGreaterThan(1);
    await freshHud(page);
    await freshHud(page);
    await expect(page.locator("#connection")).toHaveText("connected");
    const planes = assets
      .map((asset) => asset.path)
      .filter((path) =>
        /\/(?:(?:color|player|shadow)-\d+|fixture-(?:color|mask|shadow))\.png$/.test(
          path,
        ),
      );
    // CPU atlas storage already owns every destination page. Decode each
    // original source plane once rather than once per destination page.
    expect(planes.length).toBeGreaterThan(0);
    expect(planes.length).toBe(new Set(planes).size);
    expect(errors).toEqual([]);
    // Match the owner's roughly twenty-second native-pack startup before
    // the colour regression, including Firefox's PNG reconstruction cost.
    expect(Date.now() - started).toBeLessThan(20_000);
  } finally {
    await info.attach("startup", {
      contentType: "application/json",
      body: JSON.stringify({
        elapsed_ms: Date.now() - started,
        errors,
        assets,
      }),
    });
    console.log(JSON.stringify({ elapsed_ms: Date.now() - started, errors }));
  }
});
