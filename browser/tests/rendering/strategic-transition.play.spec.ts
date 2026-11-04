import { mkdir, readFile, writeFile } from "node:fs/promises";
import { gameAssets } from "../game-assets.js";
import { forceWebGl } from "./backends.js";
import { expect } from "@playwright/test";
import { test } from "../../server.js";

test("strategic presentation covers the detailed surface until a complete near frame is presented", async ({
  page,
}) => {
  await page.route("**/transition-test", (route) =>
    route.fulfill({
      contentType: "text/html",
      body: '<link rel="stylesheet" href="/interface/game.css"><canvas id="scene" style="width:1280px;height:720px"></canvas>',
    }),
  );
  await page.goto("/transition-test");
  const state = await page.evaluate(async () => {
    // @ts-expect-error Served browser module.
    const { initStrategic } = await import("/navigation/strategic.js");
    const s = initStrategic(
      { disabled: true, canvas: document.createElement("canvas"), revision: 0 },
      { records: () => [], revision: () => 0 },
    );
    const scene = document.getElementById("scene");
    if (!scene) throw new Error("Detailed canvas missing");
    const sample = () => ({
      near: getComputedStyle(scene).opacity,
      farHidden: document.getElementById("strategic-scene")?.hidden,
    });
    const samples = [sample()];
    s.frame([256, 256, 128, 128, 1280, 720, 0.04, 1]);
    samples.push(sample());
    // A camera crosses the threshold, but its detailed renderer can still be
    // waiting for terrain/art. The far surface must cover every pixel until
    // the renderer calls present after a successful complete frame.
    s.frame([256, 256, 128, 128, 1280, 720, 0.5, 0]);
    samples.push(sample());
    s.present();
    samples.push(sample());
    // Returning far must hide the retained GPU surface again.
    s.frame([256, 256, 128, 128, 1280, 720, 0.04, 1]);
    samples.push(sample());
    return samples;
  });
  expect(state).toEqual([
    { near: "1", farHidden: true },
    { near: "0", farHidden: false },
    { near: "0", farHidden: false },
    { near: "1", farHidden: true },
    { near: "0", farHidden: false },
  ]);
  // Opacity keeps the scene as the input surface, including compatibility Canvas.
  await expect(page.locator("#scene")).toHaveCSS("pointer-events", "auto");
});

test("a complete far surface replaces the live game and returns to a complete near frame", async ({
  page,
  request,
}) => {
  test.setTimeout(90_000);
  // Optional local original-art qualification uses the same assertions. Normal
  // regression runs use the disposable server and the standard art fixture.
  const config = await readFile(
    "../.cache/showcase/transition.json",
    "utf8",
  ).then(
    (text) => JSON.parse(text) as { url: string },
    () => null,
  );
  if (config) expect(config.url).toBe("http://127.0.0.1:8155/");
  else {
    expect(
      (await request.post("/maps/reset?scenario=feudal-military")).status(),
    ).toBe(204);
    await gameAssets(page);
  }
  await forceWebGl(page);
  await page.goto(
    `${config?.url ?? "/"}?play=1#x=128&y=122&zoom=.35&renderer=webgl2`,
  );
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await expect(page.locator("#scene")).toHaveCSS("opacity", "1");
  if (config) {
    await mkdir("../.cache/showcase/frames", { recursive: true });
    await page.screenshot({
      path: "../.cache/showcase/frames/direct-near-first.png",
    });
  }
  await page.evaluate(() => {
    location.hash = "x=128&y=122&zoom=.08";
  });
  await expect(page.locator("#scene")).toHaveAttribute("data-lod", "markers");
  await expect(page.locator("#scene")).toHaveCSS("opacity", "0");
  await expect(page.locator("#strategic-cockpit")).toBeVisible();
  const far = page.locator("#strategic-scene");
  await expect(far).toBeVisible();
  const box = await far.boundingBox();
  expect(box).toEqual({ x: 0, y: 0, width: 1280, height: 720 });
  if (config) {
    await page.screenshot({
      path: "../.cache/showcase/frames/direct-far-middle.png",
    });
    const state = await page.locator("#scene").evaluate((e) => ({
      ...e.dataset,
      opacity: getComputedStyle(e).opacity,
    }));
    await writeFile(
      "../.cache/showcase/direct-state.json",
      JSON.stringify({ state, box }, null, 2),
    );
  }
  await page.evaluate(() => {
    location.hash = "x=128&y=122&zoom=.35";
  });
  await expect(far).toBeHidden();
  await expect(page.locator("#scene")).toHaveCSS("opacity", "1");
  if (config) {
    await page.screenshot({
      path: "../.cache/showcase/frames/direct-near-last.png",
    });
    const png = await page
      .locator("#scene")
      .evaluate(
        (e) =>
          (e as HTMLCanvasElement).toDataURL("image/png").split(",")[1] ?? "",
      );
    await writeFile(
      "../.cache/showcase/frames/near-canvas.png",
      Buffer.from(png, "base64"),
    );
  }
});
