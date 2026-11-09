import { expect, type Page } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { activateSyntheticMap } from "../../synthetic-map.js";
import {
  CAMERA,
  look,
  open,
  presented,
  terrainServer,
  type Server,
} from "../terrain/world.js";
import { profileGpu, readProfile } from "./trace.js";

test.use({ trace: "off", screenshot: "off" });

// `window.aoePerformance.terrainPackets` is the number of terrain packets the
// client has built since the page loaded, while sampling is enabled: one for
// every ground triangle it turns into a draw-ready terrain instance (textures,
// building ground, packet, fog codes). A packet taken from the cache adds
// nothing. With a still camera only sprites change, so the count stands still.
type Counters = { frames: number; packets: number | null };

async function counters(page: Page): Promise<Counters> {
  return page.evaluate(() => {
    const packets = (
      window as Window & { aoePerformance?: { terrainPackets?: unknown } }
    ).aoePerformance?.terrainPackets;
    return {
      frames: Number(
        document.getElementById("scene")?.getAttribute("data-frame") ?? 0,
      ),
      packets: typeof packets === "number" ? packets : null,
    };
  });
}

/** Waits for the frame counter to rest, or three seconds of animation. */
async function quiet(page: Page) {
  let frames = await presented(page);
  for (let attempt = 0; attempt < 8; attempt++) {
    await page.waitForTimeout(400);
    const next = await presented(page);
    if (next === frames) return;
    frames = next;
  }
}

/** Walks the scout to and fro until `frames` world frames were presented. */
async function walk(page: Page, server: Server, frames: number) {
  const first = await presented(page);
  for (let step = 0; step < 240; step++) {
    const stride = Math.floor(step / 8) % 2 === 0 ? 0.25 : -0.25;
    server.world.tick += 1;
    server.world.scout = [
      server.world.scout[0] + stride,
      server.world.scout[1] - stride,
    ];
    server.push();
    await page.waitForTimeout(50);
    if ((await presented(page)) - first >= frames) return;
  }
}

for (const backend of ["webgpu", "webgl2"] as const) {
  test(`${backend} rebuilds no terrain packet while one unit walks under a still camera`, async ({
    page,
  }, info) => {
    test.setTimeout(180_000);
    await profileGpu(page);
    await gameAssets(page, true);
    const server = await terrainServer(page, { scout: [8189.5, 8195.5] });
    await open(page, backend, "&perf=1");
    await expect.poll(() => presented(page)).toBeGreaterThan(0);
    const plain = await presented(page);
    await activateSyntheticMap(page, false, CAMERA);
    await expect
      .poll(() => presented(page), { timeout: 20_000 })
      .toBeGreaterThan(plain);
    await quiet(page);

    await readProfile(page, true);
    const start = await counters(page);
    await walk(page, server, 8);
    const still = await counters(page);
    const stillApi = await readProfile(page, true);

    // A new camera is a new terrain picture: its packets are built once.
    await quiet(page);
    const settled = await presented(page);
    await look(page, backend, [CAMERA[0] + 2.5, CAMERA[1] - 2.5]);
    await expect(page.locator("#minimap")).toHaveAttribute(
      "data-camera-x",
      /^8195(\.0+)?$/,
    );
    await expect
      .poll(() => presented(page), { timeout: 20_000 })
      .toBeGreaterThan(settled);
    await quiet(page);
    await readProfile(page, true);
    const panned = await counters(page);
    await walk(page, server, 8);
    const again = await counters(page);
    const againApi = await readProfile(page, true);

    // Existing API counts, for the record: today every presented frame
    // uploads the whole instance buffer, terrain included.
    const result = { backend, start, still, panned, again, stillApi, againApi };
    console.log(`TERRAIN CACHE COUNT ${JSON.stringify(result)}`);
    await info.attach("terrain-cache-count", {
      body: JSON.stringify(result, null, 2),
      contentType: "application/json",
    });

    expect(
      still.frames - start.frames,
      "world frames presented while the unit walked",
    ).toBeGreaterThanOrEqual(8);
    expect(
      start.packets,
      "window.aoePerformance.terrainPackets counts the terrain packets built",
    ).not.toBeNull();
    expect(
      Number(start.packets),
      "the first frames built the terrain packets",
    ).toBeGreaterThan(0);
    expect(
      Number(still.packets) - Number(start.packets),
      "terrain packets rebuilt while only a unit moved",
    ).toBe(0);
    expect(
      Number(panned.packets),
      "a pan builds the terrain packets of its new picture",
    ).toBeGreaterThan(Number(still.packets));
    expect(again.frames - panned.frames).toBeGreaterThanOrEqual(8);
    expect(
      Number(again.packets) - Number(panned.packets),
      "terrain packets rebuilt once the panned picture was drawn",
    ).toBe(0);
  });
}
