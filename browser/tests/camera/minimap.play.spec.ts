import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";

// 64×64 raster: ocean in the west half, forest north-east, grass south-east.
function syntheticRaster(samples = 64): Buffer {
  const raster = Buffer.alloc(8 + samples * samples);
  raster.write("AWMM", 0, "latin1");
  raster.writeUInt16LE(samples, 4);
  for (let y = 0; y < samples; y++)
    for (let x = 0; x < samples; x++)
      raster[8 + y * samples + x] =
        (x < samples / 2 ? 11 : y < samples / 2 ? 2 : 0) | (8 << 4);
  return raster;
}

type Navigation = { map(hash: Uint8Array): void };

test("strategic raster is one bounded request per map and never loads the world", async ({
  page,
  request,
}) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
  const hash = "0b".repeat(32);
  const rasters: string[] = [];
  const coarse: string[] = [];
  const detailed: string[] = [];
  page.on("request", (event) => {
    if (event.url().includes(`/maps/${hash}/minimap`))
      rasters.push(event.url());
    if (event.url().includes(`/maps/${hash}/preview`)) coarse.push(event.url());
    if (event.url().includes(`/maps/${hash}/chunks/`))
      detailed.push(event.url());
  });
  await page.route(`**/maps/${hash}/minimap`, (route) =>
    route.fulfill({
      contentType: "application/octet-stream",
      body: syntheticRaster(),
    }),
  );
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected");
  // Exercise the DOM end of the production map-change bridge with a controlled raster.
  await page.evaluate(() => {
    const navigation = (window as Window & { aoeNavigation?: Navigation })
      .aoeNavigation;
    if (!navigation) throw new Error("Production navigation bridge missing");
    navigation.map(new Uint8Array(32).fill(11));
    navigation.map(new Uint8Array(32).fill(11));
  });
  await expect(page.locator("#minimap-title")).toHaveText("Minimap · terrain");
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-raster-samples",
    "64",
  );
  await expect(page.locator("#minimap-terrain")).toHaveAttribute(
    "href",
    /^data:image\/png/,
  );
  // The drawn image carries the decoded geography at raster resolution.
  const pixels = await page.evaluate(async () => {
    const image = new Image();
    image.src = document
      .getElementById("minimap-terrain")
      ?.getAttribute("href") as string;
    await image.decode();
    const canvas = document.createElement("canvas");
    canvas.width = image.naturalWidth;
    canvas.height = image.naturalHeight;
    const context = canvas.getContext("2d");
    if (!context) throw new Error("2D context missing");
    context.drawImage(image, 0, 0);
    const at = (x: number, y: number) =>
      Array.from(context.getImageData(x, y, 1, 1).data);
    return {
      size: [image.naturalWidth, image.naturalHeight],
      ocean: at(10, 30),
      forest: at(50, 10),
      grass: at(50, 50),
    };
  });
  expect(pixels).toEqual({
    size: [64, 64],
    ocean: [30, 70, 118, 255],
    forest: [34, 84, 44, 255],
    grass: [124, 164, 82, 255],
  });
  const camera = await page.locator("#world-position").innerText();
  for (let index = 0; index < 3; index++) {
    await page.getByRole("button", { name: "Show whole map" }).click();
    await expect(page.locator("#minimap")).toHaveClass(/expanded/);
    await expect(page.locator("#minimap-title")).toHaveText(
      "Whole map · terrain",
    );
    await page.keyboard.press("Escape");
    await expect(page.locator("#minimap")).not.toHaveClass(/expanded/);
  }
  expect(await page.locator("#world-position").innerText()).toBe(camera);
  await expect(page.locator("#minimap-viewport")).toHaveAttribute(
    "points",
    /^(\d+(\.\d+)?,\d+(\.\d+)?( |$)){4}/,
  );
  await expect(page.locator("#minimap-primary")).toBeVisible();
  // Clicking the detailed minimap still maps the square to the whole world.
  const panel = page.locator("#minimap");
  const width = Number(await panel.getAttribute("data-world-width"));
  const height = Number(await panel.getAttribute("data-world-height"));
  const bounds = await page.locator("#minimap-map").boundingBox();
  if (!bounds) throw new Error("minimap missing");
  const side = Math.min(bounds.width, bounds.height);
  await page.mouse.click(
    bounds.x + (bounds.width - side) / 2 + side * 0.75,
    bounds.y + (bounds.height - side) / 2 + side * 0.5,
  );
  await expect
    .poll(async () => {
      const [x, y] = (await page.locator("#world-position").innerText())
        .split(",")
        .map(Number);
      return (
        Math.abs((x ?? NaN) - width * 0.75) < width / 100 &&
        Math.abs((y ?? NaN) - height * 0.25) < height / 100
      );
    })
    .toBe(true);
  expect(rasters).toHaveLength(1);
  expect(coarse).toHaveLength(0);
  expect(detailed).toHaveLength(0);
  await page.evaluate(() => {
    const navigation = (window as Window & { aoeNavigation?: Navigation })
      .aoeNavigation;
    if (!navigation) throw new Error("Production navigation bridge missing");
    navigation.map(new Uint8Array());
  });
  await expect(page.locator("#minimap-terrain")).not.toHaveAttribute(
    "href",
    /.+/,
  );
  await expect(page.locator("#minimap-title")).toContainText("schematic");
});

test("minimap raster decoding is bounded and keeps forest apart from open land", async ({
  page,
}) => {
  await page.goto("/diagnostics.html");
  const result = await page.evaluate(async () => {
    const module = (await import("/minimap.js" as string)) as {
      MAX_MINIMAP_BYTES: number;
      MINIMAP_CLASSES: [string, number[]][];
      decodeMinimap(
        buffer: ArrayBuffer,
      ): { samples: number; rgba: Uint8ClampedArray } | null;
    };
    const raster = (samples: number, cells: number[]) => {
      const bytes = new Uint8Array(8 + cells.length);
      bytes.set([65, 87, 77, 77, samples & 255, samples >> 8, 0, 0]);
      bytes.set(cells, 8);
      return bytes;
    };
    const flat = (name: string) =>
      module.MINIMAP_CLASSES.findIndex(([label]) => label === name) | (8 << 4);
    const valid = raster(2, [
      flat("ocean"),
      flat("forest"),
      (flat("grass") & 15) | (15 << 4),
      (flat("grass") & 15) | (0 << 4),
    ]);
    const decoded = module.decodeMinimap(valid.buffer);
    const mutate = (change: (bytes: Uint8Array) => void) => {
      const copy = valid.slice();
      change(copy);
      return module.decodeMinimap(copy.buffer) === null;
    };
    const luminance = ([r, g, b]: number[]) =>
      0.2126 * (r ?? 0) + 0.7152 * (g ?? 0) + 0.0722 * (b ?? 0);
    const colors = Object.fromEntries(module.MINIMAP_CLASSES);
    const largest = module.decodeMinimap(
      raster(
        512,
        Array.from({ length: 512 * 512 }, () => flat("lake")),
      ).buffer,
    );
    return {
      classes: module.MINIMAP_CLASSES.map(([label]) => label),
      maximumBytes: module.MAX_MINIMAP_BYTES,
      samples: decoded?.samples,
      rgba: Array.from(decoded?.rgba ?? []),
      largest: [largest?.samples, largest?.rgba.length],
      rejected: [
        mutate((bytes) => (bytes[0] = 0)),
        mutate((bytes) => (bytes[6] = 1)),
        mutate((bytes) => (bytes[4] = 3)),
        mutate((bytes) => (bytes[8] = 12)),
        module.decodeMinimap(valid.slice(0, 11).buffer) === null,
        module.decodeMinimap(new ArrayBuffer(0)) === null,
        module.decodeMinimap(raster(0, []).buffer) === null,
        module.decodeMinimap(new ArrayBuffer(module.MAX_MINIMAP_BYTES + 1)) ===
          null,
        module.decodeMinimap(raster(513, []).buffer) === null,
      ],
      forestContrast:
        luminance(colors.grass ?? []) - luminance(colors.forest ?? []),
      dryContrast:
        luminance(colors["dry grass"] ?? []) - luminance(colors.forest ?? []),
      waterIsBlue: ["river", "lake", "ocean"].every((name) => {
        const [r, g, b] = colors[name] ?? [];
        return (b ?? 0) > (g ?? 0) && (g ?? 0) > (r ?? 0);
      }),
      landIsNotBlue: ["grass", "dry grass", "forest", "soil", "sand"].every(
        (name) => {
          const [, g, b] = colors[name] ?? [];
          return (b ?? 0) < (g ?? 0);
        },
      ),
    };
  });
  expect(result.classes).toEqual([
    "grass",
    "dry grass",
    "forest",
    "soil",
    "sand",
    "rock",
    "snow",
    "ice",
    "shore",
    "river",
    "lake",
    "ocean",
  ]);
  expect(result.maximumBytes).toBe(8 + 512 * 512);
  expect(result.samples).toBe(2);
  // Level ocean and forest keep their palette colour; relief lights and
  // shades grass by 4.5% per step around the level value 8.
  expect(result.rgba.slice(0, 8)).toEqual([30, 70, 118, 255, 34, 84, 44, 255]);
  expect(result.rgba.slice(8, 12)).toEqual([163, 216, 108, 255]);
  expect(result.rgba.slice(12, 16)).toEqual([79, 105, 52, 255]);
  expect(result.largest).toEqual([512, 512 * 512 * 4]);
  expect(result.rejected).toEqual(Array.from({ length: 9 }, () => true));
  expect(result.forestContrast).toBeGreaterThan(60);
  expect(result.dryContrast).toBeGreaterThan(60);
  expect(result.waterIsBlue).toBe(true);
  expect(result.landIsNotBlue).toBe(true);
});

test("minimap pointer mapping inverts the diamond rotation inside the letterboxed square", async ({
  page,
}) => {
  await page.goto("/diagnostics.html");
  const points = await page.evaluate(async () => {
    const module = (await import("/minimap.js" as string)) as {
      minimapWorldPoint(
        rect: { left: number; top: number; width: number; height: number },
        x: number,
        y: number,
        world: number[],
      ): number[] | null;
    };
    const square = { left: 100, top: 50, width: 170, height: 170 };
    const wide = { left: 0, top: 0, width: 900, height: 600 };
    const world = [20_000, 20_000];
    return [
      module.minimapWorldPoint(
        square,
        100 + 170 * 0.75,
        50 + 170 * 0.25,
        world,
      ),
      module.minimapWorldPoint(square, 100, 50, world),
      module.minimapWorldPoint(square, 0, 0, world),
      module.minimapWorldPoint(square, 9_999, 9_999, world),
      module.minimapWorldPoint(wide, 150 + 600 * 0.5, 600 * 0.1, world),
      module.minimapWorldPoint(wide, 0, 300, [500, 250]),
      module.minimapWorldPoint({ ...square, width: 0 }, 120, 60, world),
    ];
  });
  const expected = [
    [10_000, 0],
    [0, 10_000],
    [0, 15882.35294117647],
    [19_999, 15882.35294117647],
    [1999.9999999999995, 1999.9999999999995],
    [0, 249],
    null,
  ];
  points.forEach((point, index) => {
    const target = expected[index];
    if (!point || !target) expect(point).toBeNull();
    else {
      expect(point[0]).toBeCloseTo(target[0] ?? 0);
      expect(point[1]).toBeCloseTo(target[1] ?? 0);
    }
  });
});

test("blur releases held camera keys without stale impulse movement", async ({
  page,
  request,
}) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected");
  await page.mouse.move(600, 300);
  await page.keyboard.down("ArrowRight");
  await page.evaluate(() => {
    window.dispatchEvent(new Event("blur"));
  });
  await page.evaluate(
    () =>
      new Promise<void>((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
      ),
  );
  const camera = await page.locator("#world-position").innerText();
  await page.waitForTimeout(150);
  expect(await page.locator("#world-position").innerText()).toBe(camera);
  await page.keyboard.up("ArrowRight");
});
