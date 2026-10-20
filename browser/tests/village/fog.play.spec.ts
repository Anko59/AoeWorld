import { expect } from "@playwright/test";
import { readFileSync } from "node:fs";
import { PNG } from "pngjs";
import { fogState } from "../wire/fog.js";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { centerVillage, freshHud } from "./resources.js";

function blackGround(bytes: Buffer): number {
  const image = PNG.sync.read(bytes);
  let black = 0;
  for (let y = 80; y < Math.min(500, image.height); y++) {
    for (let x = 40; x < image.width - 40; x++) {
      const at = (y * image.width + x) * 4;
      if (
        (image.data[at] ?? 255) +
          (image.data[at + 1] ?? 255) +
          (image.data[at + 2] ?? 255) <
        12
      )
        black++;
    }
  }
  return black;
}

test("browser fog encoder matches the Rust golden", () => {
  const uint = (value: number) => {
    const bytes = [];
    do {
      const low = value % 128;
      value = Math.floor(value / 128);
      bytes.push(low + (value ? 128 : 0));
    } while (value);
    return bytes;
  };
  expect(
    fogState(
      uint,
      1,
      2,
      [10, 20, 13, 21],
      [0, 1, 2],
      [
        [257, 1],
        [258, 2],
      ],
    ).toString("hex"),
  ).toBe(
    readFileSync(new URL("../wire/fog.hex", import.meta.url), "utf8").trim(),
  );
});

test("the scout explores black ground and leaves persistent fog behind", async ({
  page,
  request,
}) => {
  test.setTimeout(80_000);
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page, true);
  await page.goto("/?play=1#zoom=.35");
  await expect(page.locator("#connection")).toHaveText("connected");
  const scene = page.locator("#scene");
  await expect(scene).toHaveAttribute("data-fog-disabled", "false");
  await centerVillage(page);
  const camera = await page.locator("#minimap").evaluate((el) => ({
    x: Number(el.getAttribute("data-camera-x")),
    y: Number(el.getAttribute("data-camera-y")),
  }));
  await page.evaluate(({ x, y }) => {
    location.hash = `x=${x + 10}&y=${y}&zoom=.35`;
  }, camera);
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-camera-x",
    String(camera.x + 10),
  );
  await freshHud(page);
  const count = async (name: string) =>
    Number(await scene.getAttribute(`data-fog-${name}`));
  const initial = await count("explored");
  expect(initial).toBeGreaterThan(100);
  const before = blackGround(
    await scene.screenshot({ path: "../.cache/fog-shots/village-black.png" }),
  );
  expect(before).toBeGreaterThan(10_000);
  await freshHud(page);
  const scout = (
    JSON.parse((await scene.getAttribute("data-actors")) ?? "[]") as {
      kind: string;
      x: number;
      y: number;
    }[]
  ).find((a) => a.kind === "Scout");
  if (!scout) throw new Error("Scout missing");
  await page.mouse.click(scout.x, scout.y);
  await expect(page.locator("#selection-name")).toContainText("Scout");
  // Twenty tiles east of the Town Center. The camera stays fixed for pixel comparison.
  await page.mouse.click(864, 472, { button: "right" });
  await expect
    .poll(() => count("explored"), { timeout: 35_000 })
    .toBeGreaterThan(initial + 40);
  await expect(page.locator("#unit-state")).toHaveText("unit idle", {
    timeout: 35_000,
  });
  await expect
    .poll(async () => (await count("explored")) - (await count("visible")))
    .toBeGreaterThan(40);
  const explored = await count("explored");
  await page.waitForTimeout(1000);
  expect(await count("explored")).toBe(explored);
  await expect(page.locator("#minimap-map")).toHaveAttribute(
    "data-fog",
    "enabled",
  );
  const after = blackGround(
    await scene.screenshot({ path: "../.cache/fog-shots/scout-explored.png" }),
  );
  expect(after).toBeLessThan(before - 1000);

  // A new connection and a fresh subscription must restore the same memory.
  // Keep the camera fixed so these counts cover exactly the same tile region.
  await page.reload();
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect(scene).toHaveAttribute("data-fog-disabled", "false");
  await expect.poll(() => count("explored")).toBe(explored);
  await expect
    .poll(async () => (await count("explored")) - (await count("visible")))
    .toBeGreaterThan(40);
  await page.evaluate(({ x, y }) => {
    location.hash = `x=${x + 256}&y=${y}&zoom=.35`;
  }, camera);
  await expect.poll(() => count("explored")).toBe(0);
  await page.evaluate(({ x, y }) => {
    location.hash = `x=${x + 10}&y=${y}&zoom=.35`;
  }, camera);
  await expect.poll(() => count("explored")).toBe(explored);
});
