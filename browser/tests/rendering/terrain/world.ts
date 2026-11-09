import { expect, type Page, type WebSocketRoute } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";
import { PNG } from "pngjs";
import { position, uint } from "../../village/input/fixture.js";
import { darkAgeState, playerEconomyTail } from "../../wire/buildings.js";
import { scoutCombat } from "../../wire/combat.js";
import { fogState } from "../../wire/fog.js";

// A frozen, scripted server for the terrain-cache specs: one scout, optional
// buildings and fog. Every pixel check compares two views of the same run, so
// no colour or image is pinned across machines.
export const BACKENDS = ["webgpu", "webgl2", "canvas2d"] as const;
export type Backend = (typeof BACKENDS)[number];
export const CAMERA = [8192.5, 8192.5] as const;
export const ZOOM = 0.6;
/** World pixels clear of the top bar, the bottom panel and the side widgets. */
export const CLIP = { x: 240, y: 90, width: 800, height: 390 } as const;
export type Box = { x: number; y: number; width: number; height: number };

const FOG_REGION = [8160, 8160, 8224, 8224] as const;
const WELCOME = Buffer.from([
  0,
  4,
  1,
  0,
  0,
  ...uint(32768),
  ...uint(32768),
  ...uint(1024),
  20,
  0,
  ...Array<number>(24).fill(1),
]);

export type World = {
  tick: number;
  scout: [number, number];
  buildings: number[][];
  /** Sight radius in tiles; `null` sends the protocol's disabled fog. */
  sight: number | null;
  explored: Set<number>;
};

export type Server = {
  world: World;
  /** Sends the current world as a snapshot and its fog. */
  push(): void;
  /** Sends one raw frame for the client's current subscription revision. */
  frame(build: (revision: number[]) => number[]): void;
};

function snapshot(world: World, revision: number[]): Buffer {
  return Buffer.from([
    1,
    ...revision,
    ...uint(world.tick),
    1,
    0,
    0,
    ...position(world.scout[0], world.scout[1]),
    0,
    0,
    0,
    1,
    0,
    0,
    0,
    ...scoutCombat,
    1,
    0,
    ...uint(2000),
    ...uint(2000),
    100,
    ...uint(2000),
    1,
    20,
    ...darkAgeState,
    ...playerEconomyTail,
    world.buildings.length,
    ...world.buildings.flat(),
  ]);
}

function fog(world: World, revision: number[]): Buffer {
  const subscription = revision.reduce(
    (value, byte, index) => value + (byte & 127) * 128 ** index,
    0,
  );
  if (world.sight === null)
    return fogState(uint, subscription, world.tick, [0, 0, 1, 1], [], [], true);
  const [x0, y0, x1, y1] = FOG_REGION;
  const tiles: number[] = [];
  for (let y = y0; y < y1; y++)
    for (let x = x0; x < x1; x++) {
      const index = (y - y0) * (x1 - x0) + (x - x0);
      const visible =
        (x + 0.5 - world.scout[0]) ** 2 + (y + 0.5 - world.scout[1]) ** 2 <=
        world.sight ** 2;
      if (visible) world.explored.add(index);
      tiles.push(visible ? 2 : world.explored.has(index) ? 1 : 0);
    }
  return fogState(uint, subscription, world.tick, [x0, y0, x1, y1], tiles);
}

/** Answers every subscription with the current world, also after a reload. */
export async function terrainServer(
  page: Page,
  start: { scout: [number, number]; sight?: number },
): Promise<Server> {
  const world: World = {
    tick: 1,
    scout: start.scout,
    buildings: [],
    sight: start.sight ?? null,
    explored: new Set(),
  };
  let socket: WebSocketRoute | undefined;
  let revision: number[] = [];
  const push = () => {
    if (!socket || revision.length === 0) return;
    socket.send(snapshot(world, revision));
    socket.send(fog(world, revision));
  };
  await page.routeWebSocket("**/game/ws", (route) => {
    socket = route;
    revision = [];
    route.onMessage((message) => {
      const bytes =
        typeof message === "string" ? Buffer.from(message) : message;
      if (bytes[0] === 0) route.send(WELCOME);
      if (bytes[0] !== 1) return;
      revision = [];
      for (const byte of bytes.subarray(1)) {
        revision.push(byte);
        if (byte < 128) break;
      }
      push();
    });
  });
  return {
    world,
    push,
    frame(build) {
      if (socket && revision.length > 0)
        socket.send(Buffer.from(build(revision)));
    },
  };
}

/** Where flat ground at a world position is drawn (1280 by 720 viewport). */
export function screen(
  x: number,
  y: number,
  camera: readonly [number, number] = CAMERA,
) {
  const dx = x - camera[0];
  const dy = y - camera[1];
  return { x: 640 + (dx - dy) * 64 * ZOOM, y: 360 + (dx + dy) * 32 * ZOOM };
}

/** A ten pixel square around a screen point. */
export function around(point: { x: number; y: number }): Box {
  return {
    x: Math.round(point.x) - 5,
    y: Math.round(point.y) - 5,
    width: 10,
    height: 10,
  };
}

async function connected(page: Page, timeout: number): Promise<boolean> {
  try {
    await expect(page.locator("#connection")).toHaveText("connected", {
      timeout,
    });
    return true;
  } catch {
    return false;
  }
}

/** Waits for the connected game on the requested tier. Under load a software
 * GPU browser can stall while the page starts, or miss the WebGPU startup
 * deadline and fall back to WebGL2 (both seen before any terrain was drawn):
 * reload up to twice before requiring the connection and the tier. */
export async function ready(page: Page, backend: Backend) {
  const playground = page.locator("#playground");
  for (let attempt = 0; attempt < 2; attempt++) {
    if (
      (await connected(page, 25_000)) &&
      (await playground.getAttribute("data-renderer")) === backend
    )
      break;
    await page.reload();
  }
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await expect(playground).toHaveAttribute("data-renderer", backend);
}

export async function open(page: Page, backend: Backend, query = "") {
  const renderer = backend === "canvas2d" ? "canvas" : backend;
  await page.goto(
    `/?play=1${query}#x=${CAMERA[0]}&y=${CAMERA[1]}&zoom=${ZOOM}&renderer=${renderer}`,
  );
  await ready(page, backend);
}

/** Moves the camera through its link fragment, without touching the pointer. */
export async function look(
  page: Page,
  backend: Backend,
  camera: readonly [number, number],
) {
  const renderer = backend === "canvas2d" ? "canvas" : backend;
  await page.evaluate((hash) => {
    location.hash = hash;
  }, `x=${camera[0]}&y=${camera[1]}&zoom=${ZOOM}&renderer=${renderer}`);
}

/** World frames the client has presented so far (`#scene[data-frame]`). */
export async function presented(page: Page): Promise<number> {
  return Number(await page.locator("#scene").getAttribute("data-frame"));
}

export async function shot(page: Page): Promise<Buffer> {
  return page.screenshot({ clip: CLIP });
}

/** The view once two consecutive captures agree. */
export async function stableShot(page: Page): Promise<Buffer> {
  let previous = await shot(page);
  for (let attempt = 0; attempt < 60; attempt++) {
    await page.waitForTimeout(200);
    const next = await shot(page);
    if (next.equals(previous)) return next;
    previous = next;
  }
  throw new Error("The world view never settled");
}

/** Pixels that differ between two captures, outside `ignore` (page pixels). */
export function changes(left: Buffer, right: Buffer, ignore?: Box): number {
  if (left.equals(right)) return 0;
  const a = PNG.sync.read(left);
  const b = PNG.sync.read(right);
  let count = 0;
  for (let y = 0; y < a.height; y++)
    for (let x = 0; x < a.width; x++) {
      if (ignore) {
        const px = x + CLIP.x - ignore.x;
        const py = y + CLIP.y - ignore.y;
        if (px >= 0 && py >= 0 && px < ignore.width && py < ignore.height)
          continue;
      }
      const at = (y * a.width + x) * 4;
      if (
        a.data[at] !== b.data[at] ||
        a.data[at + 1] !== b.data[at + 1] ||
        a.data[at + 2] !== b.data[at + 2]
      )
        count++;
    }
  return count;
}

/** Mean red, green and blue of a box given in page pixels. */
export function mean(image: Buffer, box: Box = CLIP): [number, number, number] {
  const png = PNG.sync.read(image);
  let red = 0;
  let green = 0;
  let blue = 0;
  for (let y = box.y; y < box.y + box.height; y++)
    for (let x = box.x; x < box.x + box.width; x++) {
      const at = ((y - CLIP.y) * png.width + (x - CLIP.x)) * 4;
      red += png.data[at] ?? 0;
      green += png.data[at + 1] ?? 0;
      blue += png.data[at + 2] ?? 0;
    }
  const pixels = box.width * box.height;
  return [red / pixels, green / pixels, blue / pixels];
}

export const luminance = ([red, green, blue]: [number, number, number]) =>
  (red + green + blue) / 3;

/** Green minus red: positive on grass, negative on cleared or ploughed dirt. */
export const greenness = ([red, green]: [number, number, number]) =>
  green - red;

async function keep(name: string, images: Record<string, Buffer>) {
  const directory = "../.cache/terrain-cache";
  await mkdir(directory, { recursive: true });
  const stem = name.replace(/[^a-z0-9]+/gi, "-").toLowerCase();
  for (const [kind, image] of Object.entries(images))
    await writeFile(`${directory}/${stem}-${kind}.png`, image);
}

/** Waits until the view equals `expected` pixel for pixel. */
export async function expectView(
  page: Page,
  expected: Buffer,
  label: string,
  ignore?: Box,
) {
  let actual = expected;
  try {
    await expect
      .poll(
        async () => {
          actual = await shot(page);
          return changes(actual, expected, ignore);
        },
        { message: `${label}: differing pixels`, timeout: 20_000 },
      )
      .toBe(0);
  } catch (error) {
    await keep(label, { expected, actual });
    throw error;
  }
}

/** Waits until the view differs from `previous`, then returns it settled. */
export async function changedView(
  page: Page,
  previous: Buffer,
  label: string,
): Promise<Buffer> {
  await expect
    .poll(async () => changes(await shot(page), previous), {
      message: `${label}: changed pixels`,
      timeout: 20_000,
    })
    .toBeGreaterThan(0);
  return stableShot(page);
}

/** Keeps a full-page capture beside the reports for a human to look at. */
export async function evidence(page: Page, name: string) {
  await keep(name, { page: await page.screenshot() });
}
