import { expect, type Page } from "@playwright/test";
import { test } from "../server.js";
import { gameAssets } from "./game-assets.js";
import { denyWebGl } from "./rendering/backends.js";
import { syntheticTerrainCoverage } from "./surface-evidence.js";
import { PNG } from "pngjs";
import { freshHud } from "./village/fixtures/resources.js";
import {
  bluePixels,
  assetTerrainColorVariety,
} from "./rendering/play-pixels.js";

async function waitForGame(page: Page) {
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-assets",
    "aoe2-local",
  );
}

test.beforeEach(async ({ request }) => {
  const response = await request.post("/maps/reset");
  expect(response.status()).toBe(204);
});

test("authoritative isometric game renders, selects, orders, and survives reload", async ({
  page,
}, testInfo) => {
  const evidence = await gameAssets(page);
  testInfo.annotations.push({ type: "assets", description: evidence });
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/?play=1");
  await waitForGame(page);
  await page.keyboard.press("Home");
  if (testInfo.project.name === "webgpu") {
    await expect(page.locator("#playground")).toHaveAttribute(
      "data-renderer",
      "webgpu",
    );
  }
  const canvas = page.locator("#scene");
  await expect
    .poll(() => bluePixels(canvas), { timeout: 10_000 })
    .toBeGreaterThan(10);
  const first = await canvas.screenshot();
  const image = PNG.sync.read(first);
  let green = 0;
  let blue = 0;
  let terrainTiles = 0;
  for (let i = 0; i < image.data.length; i += 4) {
    const r = image.data[i] ?? 0;
    const g = image.data[i + 1] ?? 0;
    const b = image.data[i + 2] ?? 0;
    const y = Math.floor(i / 4 / image.width);
    if (y < 54 || y >= image.height - 208) continue;
    if (g > r + 8 && g > b + 4) green += 1;
    if (b > 150 && b > r + 30) blue += 1;
    if (r >= 60 && r <= 80 && g > 100 && b >= 40 && b <= 70) terrainTiles += 1;
  }
  expect(green).toBeGreaterThan(image.width * (image.height - 54 - 208) * 0.6);
  expect(blue).toBeGreaterThan(10);
  if (evidence === "generated CI fixtures") {
    expect(terrainTiles).toBeGreaterThan(
      image.width * (image.height - 54 - 208) * 0.1,
    );
  } else {
    const terrainColors = assetTerrainColorVariety(image);
    testInfo.annotations.push({
      type: "asset terrain colors",
      description: `${terrainColors} distinct sampled terrain colors`,
    });
    expect(terrainColors).toBeGreaterThan(12);
  }

  const box = await canvas.boundingBox();
  if (!box) throw new Error("Missing map");

  if (evidence === "generated CI fixtures") {
    await freshHud(page);
    const town = await canvas.evaluate((element) =>
      (
        JSON.parse(element.getAttribute("data-actors") ?? "[]") as {
          kind: string;
          x: number;
          y: number;
        }[]
      ).find((actor) => actor.kind === "TownCenter"),
    );
    if (!town) throw new Error("Starting Town Center missing");
    // At normal zoom this ground northeast of the Town Center is fully within
    // its eight-tile LOS and clear of buildings, actors and their shadows.
    // Exact source colors belong on unoccluded, revealed ground; the far view
    // below also contains correctly black unexplored terrain.
    expect(
      syntheticTerrainCoverage(image, {
        left: Math.floor(town.x + 220),
        right: Math.floor(town.x + 340),
        top: Math.floor(town.y - 220),
        bottom: Math.floor(town.y - 140),
      }),
    ).toBeGreaterThan(0.85);
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.wheel(0, 5_000);
    await page.waitForTimeout(100);
    expect(await bluePixels(canvas)).toBeGreaterThan(10);
    const zoomed = PNG.sync.read(await canvas.screenshot());
    const unexplored = (96 * zoomed.width + 64) * 4;
    expect([...zoomed.data.subarray(unexplored, unexplored + 3)]).toEqual([
      0, 0, 0,
    ]);
  }

  await canvas.hover({
    position: { x: box.width / 2, y: box.height / 2 },
  });
  await expect(page.locator("#tile-inspection")).toContainText(
    "tile 8192, 8192",
  );
  await canvas.click({ position: { x: box.width / 2, y: box.height / 2 } });
  await canvas.click({
    position: { x: box.width / 2 + 110, y: box.height / 2 + 30 },
    button: "right",
  });
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 10_000,
  });
  await page.getByRole("button", { name: "Toggle isometric grid" }).click();
  await page.getByRole("button", { name: "Center on primary unit" }).click();
  await page.reload();
  await waitForGame(page);
  await page.keyboard.press("Home");
  expect(errors).toEqual([]);
});

test("rapid viewport changes ignore stale subscription frames", async ({
  page,
}) => {
  await gameAssets(page);
  await page.addInitScript(() => {
    const NativeWebSocket = window.WebSocket;
    class ProbedWebSocket extends NativeWebSocket {
      constructor(...args: ConstructorParameters<typeof WebSocket>) {
        super(...args);
        let handler: ((event: MessageEvent) => void) | null = null;
        let queued: MessageEvent | null = null;
        let sends = 0;
        const descriptor = Object.getOwnPropertyDescriptor(
          NativeWebSocket.prototype,
          "onmessage",
        );
        if (!descriptor) return;
        Object.defineProperty(this, "onmessage", {
          configurable: true,
          get: () => handler,
          set: (value: ((event: MessageEvent) => void) | null) => {
            handler = value;
            descriptor.set?.call(
              this,
              value &&
                ((event: MessageEvent) => {
                  if (sends >= 2 && queued === null) {
                    queued = event;
                    return;
                  }
                  value(event);
                }),
            );
          },
        });
        const nativeSend = this.send.bind(this);
        this.send = (
          data: string | ArrayBufferLike | Blob | ArrayBufferView,
        ) => {
          sends += 1;
          const result = nativeSend(data);
          if (sends >= 3 && queued !== null) {
            const stale = queued;
            queued = null;
            setTimeout(() => {
              (
                window as Window & { __staleInjected?: boolean }
              ).__staleInjected = true;
              handler?.(new MessageEvent("message", { data: stale.data }));
            }, 0);
          }
          return result;
        };
      }
    }
    window.WebSocket = ProbedWebSocket;
  });
  await page.goto("/?play=1");
  await waitForGame(page);
  await page.keyboard.press("Home");
  const canvas = page.locator("#scene");
  const box = await canvas.boundingBox();
  if (!box) throw new Error("Missing map");
  const center = {
    x: box.x + box.width / 2,
    y: box.y + box.height / 2,
  };
  const pans: Array<[number, number]> = [
    [120, 40],
    [-180, 70],
    [90, -100],
    [-140, -50],
    [160, 80],
    [-110, 30],
  ];
  for (const [dx, dy] of pans) {
    await page.mouse.move(center.x, center.y);
    await page.mouse.down({ button: "middle" });
    await page.mouse.move(center.x + dx, center.y + dy);
    await page.mouse.up({ button: "middle" });
  }
  await expect
    .poll(
      () =>
        page.evaluate(
          () =>
            (window as Window & { __staleInjected?: boolean }).__staleInjected,
        ),
      { timeout: 10_000 },
    )
    .toBe(true);
  await expect
    .poll(() => page.locator("#connection").textContent(), { timeout: 10_000 })
    .not.toContain("protocol error");
});

for (const failure of ["missing-api", "null-context", "no-adapter"] as const) {
  for (const software of [false, true]) {
    test(`game remains playable after WebGPU ${failure}${software ? " with WebGL2 denied" : ""}`, async ({
      page,
    }) => {
      await gameAssets(page);
      if (software) await denyWebGl(page);
      await page.addInitScript((mode) => {
        if (mode === "missing-api") {
          Object.defineProperty(navigator, "gpu", { value: undefined });
        } else if (mode === "no-adapter") {
          const gpu = (navigator as Navigator & { gpu?: object }).gpu;
          if (gpu)
            Object.defineProperty(gpu, "requestAdapter", {
              value: async () => null,
            });
        } else {
          const original = HTMLCanvasElement.prototype.getContext;
          HTMLCanvasElement.prototype.getContext = function (
            this: HTMLCanvasElement,
            type: string,
            ...args: unknown[]
          ) {
            if (type === "webgpu") return null;
            return Reflect.apply(original, this, [type, ...args]);
          } as typeof original;
        }
      }, failure);
      await page.goto("/?play=1");
      await waitForGame(page);
      await page.keyboard.press("Home");
      await expect(page.locator("#playground")).toHaveAttribute(
        "data-renderer",
        software ? "canvas2d" : /^(webgl2|canvas2d)$/,
      );
      await expect(page.getByRole("alert")).toBeEmpty();
    });
  }
}

test("missing local assets show an actionable error", async ({ page }) => {
  await page.route("**/asset-pack/manifest.json", (route) =>
    route.fulfill({ status: 404 }),
  );
  await page.goto("/?play=1");
  await expect(page.getByRole("alert")).toContainText("local asset pack");
  await expect(page.locator("#connection")).toHaveText("Unavailable");
});

test("unavailable renderers stop loading with a visible error", async ({
  page,
}) => {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "gpu", { value: undefined });
    HTMLCanvasElement.prototype.getContext = () => null;
  });
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("Unavailable");
  await expect(page.getByRole("alert")).toContainText(
    "Canvas 2D is unavailable",
  );
});
