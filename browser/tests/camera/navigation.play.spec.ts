import { expect, test, type Page } from "@playwright/test";
import { PNG } from "pngjs";
import { gameAssets } from "../game-assets.js";
import { activateSyntheticMap } from "../synthetic-map.js";
import { forceCanvas } from "../rendering/backends.js";

async function position(page: Page): Promise<[number, number]> {
  return (await page.locator("#world-position").innerText())
    .split(",")
    .map(Number) as [number, number];
}

function unsigned(value: number): number[] {
  const bytes: number[] = [];
  do {
    const digit = value % 128;
    value = Math.floor(value / 128);
    bytes.push(digit + (value ? 128 : 0));
  } while (value);
  return bytes;
}

async function wideWorld(page: Page) {
  // Postcard protocol-8 fixture: a stationary primary and a 70,000-square world.
  // No shipping test export or source assets are required for the grid regression.
  const coordinate = unsigned((35_000 * 1024 + 512) * 2);
  const welcome = Buffer.from([
    0,
    8,
    1,
    0,
    0,
    ...unsigned(140_000),
    ...unsigned(140_000),
    ...unsigned(1024),
    20,
    0,
    1,
    0,
  ]);
  await page.routeWebSocket("**/game/ws", (socket) => {
    socket.onMessage((message) => {
      const bytes =
        typeof message === "string" ? Buffer.from(message) : message;
      if (bytes[0] === 0) socket.send(welcome);
      if (bytes[0] !== 0 && bytes[0] !== 1) return;
      const revision: number[] = [];
      if (bytes[0] === 1) {
        for (let index = 1; index < bytes.length; index++) {
          const byte = bytes[index] ?? 0;
          revision.push(byte);
          if (byte < 128) break;
        }
      } else revision.push(0);
      socket.send(
        Buffer.from([
          1,
          ...revision,
          0,
          1,
          1,
          0,
          ...coordinate,
          ...coordinate,
          0,
          0,
          4,
        ]),
      );
    });
  });
}

function changedTerrainPixels(before: PNG, after: PNG): number {
  let changed = 0;
  // Exclude the horse, minimap, toolbar and status: count real terrain-grid pixels.
  for (let y = 110; y < 400; y++) {
    for (let x = 150; x < 450; x++) {
      const offset = (y * before.width + x) * 4;
      if (
        [0, 1, 2].some(
          (channel) =>
            Math.abs(
              (before.data[offset + channel] ?? 0) -
                (after.data[offset + channel] ?? 0),
            ) > 8,
        )
      )
        changed++;
    }
  }
  return changed;
}

test("grid button draws and removes terrain grid beyond default world bounds", async ({
  page,
}, testInfo) => {
  if (testInfo.project.name === "browser-defaults") {
    await forceCanvas(page);
  }
  await gameAssets(page);
  await wideWorld(page);
  await page.goto("/");
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-world-width",
    "70000",
  );
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    testInfo.project.name === "webgpu" ? "webgpu" : "canvas2d",
  );
  await activateSyntheticMap(page, false, [35_000.5, 35_000.5]);
  await page.getByRole("button", { name: "Center on primary unit" }).click();
  await expect.poll(() => position(page)).toEqual([35_000.5, 35_000.5]);
  await page.mouse.move(600, 300);
  const scene = page.locator("#scene");
  const before = PNG.sync.read(await scene.screenshot());
  await page.getByRole("button", { name: "Toggle isometric grid" }).click();
  await expect
    .poll(async () =>
      changedTerrainPixels(before, PNG.sync.read(await scene.screenshot())),
    )
    .toBeGreaterThan(400);
  await page.getByRole("button", { name: "Toggle isometric grid" }).click();
  await expect
    .poll(async () =>
      changedTerrainPixels(before, PNG.sync.read(await scene.screenshot())),
    )
    .toBeLessThan(30);
});

test("whole-map navigation jumps locally, returns home, and held keys release", async ({
  page,
  request,
}) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
  await page.goto("/");
  await expect(page.locator("#connection")).toHaveText("connected");
  await page.getByRole("button", { name: "Center on primary unit" }).click();
  await expect.poll(() => position(page)).toEqual([8192.5, 8192.5]);
  const start = await position(page);
  await page.mouse.move(600, 300);
  await page.keyboard.down("ArrowRight");
  await expect
    .poll(async () => (await position(page))[0] - start[0])
    .toBeGreaterThan(2);
  await page.keyboard.up("ArrowRight");
  await expect
    .poll(() => page.locator("#world-position").textContent())
    .toBeTruthy();
  const stopped = await position(page);
  await page.waitForTimeout(150);
  await expect.poll(() => position(page)).toEqual(stopped);
  await page.getByRole("button", { name: "Show whole map" }).click();
  await expect(page.locator("#minimap")).toHaveClass(/expanded/);
  const map = page.locator("#minimap-map");
  const bounds = await map.boundingBox();
  if (!bounds) throw new Error("Minimap missing");
  const side = Math.min(bounds.width, bounds.height);
  await page.mouse.click(
    bounds.x + (bounds.width - side) / 2 + side * 0.75,
    bounds.y + (bounds.height - side) / 2 + side * 0.25,
  );
  await expect
    .poll(async () => {
      const [x, y] = await position(page);
      return Math.abs(x - 12_288) < 40 && Math.abs(y - 4096) < 40;
    })
    .toBe(true);
  await page.getByRole("button", { name: "Close map overview" }).click();
  await expect(page.locator("#minimap")).not.toHaveClass(/expanded/);
  await page.keyboard.press("Home");
  await expect.poll(() => position(page)).toEqual(start);
  await page.getByRole("button", { name: "Open map creator" }).click();
  await page.locator("#map-seed").focus();
  const typingStart = await position(page);
  await page.keyboard.press("ArrowRight");
  await page.waitForTimeout(150);
  await expect.poll(() => position(page)).toEqual(typingStart);
});
