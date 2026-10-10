import { expect, test, type Page } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";
import { PNG } from "pngjs";
import { forceCanvas, forceWebGl } from "../rendering/backends.js";
import { processMemory } from "../memory/process.js";

const hash = process.env["AOE_COUNTRY_SOURCE_HASH"];
const assetHash =
  "7e6fa0da194d13fcff0cd50e74fe92215fd11b5bcce4556e4dad3d34ff7447ce";
const directory = process.env["AOE_COUNTRY_SOURCE_OUTPUT"];
type WireFrame = { direction: "sent" | "received"; payload_hex: string };
function outputDirectory() {
  if (!directory)
    throw new Error("missing current-run country source output directory");
  return directory;
}
test.skip(!hash, "explicit original-pack candidate qualification only");
test.setTimeout(600_000);

async function ready(page: Page) {
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-assets",
    "aoe2-local",
    { timeout: 60_000 },
  );
  const manifest = await page.evaluate(async () => {
    const response = await fetch("/asset-pack/manifest.json");
    if (!response.ok)
      throw new Error(
        "missing original asset manifest; generated fallback prohibited",
      );
    return response.json();
  });
  expect(manifest.version).toBe(1);
  expect(manifest.input_hash).toBe(assetHash);
  return { version: manifest.version, input_hash: manifest.input_hash };
}

function imageEvidence(image: Buffer) {
  const png = PNG.sync.read(image);
  const colors = new Set<number>();
  for (let i = 0; i < png.data.length; i += 4)
    colors.add(
      ((png.data[i] ?? 0) << 16) |
        ((png.data[i + 1] ?? 0) << 8) |
        (png.data[i + 2] ?? 0),
    );
  expect(colors.size).toBeGreaterThan(50);
  expect(png.width * png.height).toBeLessThanOrEqual(4_194_304);
  return { width: png.width, height: png.height, distinctColors: colors.size };
}

test("isolated original-pack country renders actual chunks on three explicit backends", async ({
  page,
  browser,
}) => {
  if (!hash) throw new Error("missing explicit country hash");
  expect(hash).toMatch(/^[0-9a-f]{64}$/);
  await mkdir(outputDirectory(), { recursive: true });
  const wireFrames: WireFrame[] = [];
  let captureWire = false;
  let captureFailure: string | null = null;
  let wireBytes = 0;
  page.on("websocket", (socket) => {
    if (new URL(socket.url()).pathname !== "/game/ws") return;
    const retain = (
      direction: WireFrame["direction"],
      payload: string | Buffer,
    ) => {
      if (!captureWire) return;
      const bytes = Buffer.from(payload);
      wireBytes += bytes.byteLength;
      if (
        bytes.byteLength > 1_048_576 ||
        wireBytes > 8_388_608 ||
        wireFrames.length >= 8192
      ) {
        captureFailure = "live wire evidence exceeded its declared bound";
        captureWire = false;
        return;
      }
      wireFrames.push({ direction, payload_hex: bytes.toString("hex") });
    };
    socket.on("framesent", ({ payload }) => retain("sent", payload));
    socket.on("framereceived", ({ payload }) => retain("received", payload));
  });
  await page.goto("/");
  const manifest = await ready(page);
  await page.getByRole("button", { name: "Open map creator" }).click();
  await page.getByLabel("Saved maps").selectOption(hash);
  await page.getByRole("button", { name: "Open saved" }).click();
  await expect(page.locator("#map-estimate")).toContainText(
    `Saved map active: ${hash.slice(0, 12)}`,
    { timeout: 60_000 },
  );
  await expect(page.locator("#connection")).toHaveText(/reconnect/);
  captureWire = true;
  await page.getByRole("button", { name: "Reconnect to server" }).click();
  await ready(page);

  // Fixed browser-order observation: select the primary unit and issue one right
  // click at the declared screen offset. Rust later decodes the real WS frames.
  await page.getByRole("button", { name: "Center on primary unit" }).click();
  const liveCanvas = page.locator("#scene");
  const liveBox = await liveCanvas.boundingBox();
  if (!liveBox) throw new Error("Missing active candidate scene");
  const center = { x: liveBox.width / 2, y: liveBox.height / 2 };
  await liveCanvas.hover({ position: center });
  const tileAt = async (x: number, y: number) => {
    await liveCanvas.hover({ position: { x, y } });
    const text = (await page.locator("#tile-inspection").textContent()) ?? "";
    const match = /tile (-?\d+), (-?\d+)/.exec(text);
    if (!match) throw new Error(`Could not inspect candidate tile: ${text}`);
    return { x: Number(match[1]), y: Number(match[2]), text };
  };
  const originTile = await tileAt(center.x, center.y);
  await liveCanvas.click({ position: center });
  await expect(page.locator("#unit-state")).toHaveText("unit idle");
  const destination = await tileAt(center.x + 110, center.y + 30);
  expect(destination.text).toMatch(/\bpassable\b/);
  expect([destination.x, destination.y]).not.toEqual([
    originTile.x,
    originTile.y,
  ]);
  await liveCanvas.click({
    position: { x: center.x + 110, y: center.y + 30 },
    button: "right",
  });
  await expect(page.locator("#unit-state")).toHaveText("unit moving", {
    timeout: 30_000,
  });
  await expect(page.locator("#unit-state")).toHaveText("unit idle", {
    timeout: 120_000,
  });
  captureWire = false;
  if (captureFailure) throw new Error(captureFailure);
  await writeFile(
    `${outputDirectory()}/live-wire.json`,
    JSON.stringify({
      content_hash: hash,
      origin_tile: [originTile.x, originTile.y],
      destination_tile: [destination.x, destination.y],
      fixed_screen_offset: [110, 30],
      frame_count: wireFrames.length,
      payload_bytes: wireBytes,
      frames: wireFrames,
    }),
  );

  await page.getByRole("button", { name: "Open map creator" }).click();
  const observations: unknown[] = [];
  const rendererErrors: string[] = [];
  let retiredPage: Page | null = null;
  let current = page;
  for (const backend of ["webgpu", "webgl2", "canvas2d"] as const) {
    const fetched = new Set<string>();
    const errors: string[] = [];
    current.on("response", (response) => {
      const path = new URL(response.url()).pathname;
      if (path.startsWith(`/maps/${hash}/chunks/`) && response.ok())
        fetched.add(path);
    });
    current.on("pageerror", (error) => {
      errors.push(error.message);
      rendererErrors.push(error.message);
    });
    await current.goto("/");
    await ready(current);
    if (retiredPage) {
      await retiredPage.close();
      retiredPage = null;
    }
    await expect(current.locator("#playground")).toHaveAttribute(
      "data-renderer",
      backend,
      { timeout: 60_000 },
    );
    await expect(current.locator("#minimap-primary")).toHaveAttribute(
      "visibility",
      "visible",
    );
    await current.keyboard.press("Home");
    // Real response paths, not a forged cache counter or generated art fixture.
    await current.mouse.move(640, 360);
    await current.mouse.down({ button: "middle" });
    await current.mouse.move(440, 360);
    await current.mouse.up({ button: "middle" });
    await expect
      .poll(() => fetched.size, { timeout: 60_000 })
      .toBeGreaterThan(0);
    await current.waitForLoadState("networkidle");
    const cache = await current.locator("#terrain-cache").innerText();
    const resident = Number(/\((\d+) \/ 512 chunks\)/.exec(cache)?.[1]);
    expect(resident).toBeGreaterThan(0);
    expect(resident).toBeLessThanOrEqual(512);
    const image = await current.locator("#scene").screenshot();
    const pixels = imageEvidence(image);
    await writeFile(`${directory}/${backend}.png`, image);
    await current.mouse.move(640, 360);
    await current.mouse.wheel(0, 5_000);
    await current.waitForLoadState("networkidle");
    const wide = await current.locator("#scene").screenshot();
    const widePixels = imageEvidence(wide);
    await writeFile(`${directory}/${backend}-wide.png`, wide);
    const wideCache = await current.locator("#terrain-cache").innerText();
    const wideResident = Number(/\((\d+) \/ 512 chunks\)/.exec(wideCache)?.[1]);
    expect(wideResident).toBeGreaterThan(0);
    expect(wideResident).toBeLessThanOrEqual(512);
    expect(errors).toEqual([]);
    observations.push({
      backend,
      content_hash: hash,
      asset_manifest: manifest,
      pixels,
      widePixels,
      wideCache,
      fetched_chunks: [...fetched].sort(),
      cache,
      camera: await current.locator("#world-position").innerText(),
      errors,
    });
    if (backend !== "canvas2d") {
      const token = await current.evaluate(() =>
        sessionStorage.getItem("aoeworld.resume-token"),
      );
      if (!token) throw new Error("missing controller resume token");
      const origin = new URL(current.url()).origin;
      const previous = current;
      await current.goto("about:blank");
      const next = await browser.newPage();
      await next.addInitScript(
        ({ origin, token }) => {
          if (location.origin === origin)
            sessionStorage.setItem("aoeworld.resume-token", token);
        },
        { origin, token },
      );
      if (backend === "webgpu") await forceWebGl(next);
      else await forceCanvas(next);
      retiredPage = previous;
      current = next;
    }
  }

  const memorySession = await browser.newBrowserCDPSession();
  const memoryChunks = new Set<string>();
  current.on("response", (response) => {
    const path = new URL(response.url()).pathname;
    if (path.startsWith(`/maps/${hash}/chunks/`) && response.ok())
      memoryChunks.add(path);
  });
  await current.keyboard.press("Home");
  await current.mouse.move(640, 360);
  await current.mouse.wheel(0, 5_000);
  await current.waitForLoadState("networkidle");
  const origin = await current.locator("#world-position").innerText();
  const memorySamples: {
    residentBytes: number;
    camera: string;
    cache: string;
  }[] = [];
  for (let step = 0; step < 24; step++) {
    const dx = step < 12 ? -8_000 : 8_000;
    await current.mouse.move(640, 360);
    await current.mouse.down({ button: "middle" });
    await current.mouse.move(640 + dx, 360);
    await current.mouse.up({ button: "middle" });
    await current.mouse.move(640, 360);
    await current.waitForLoadState("networkidle");
    await expect(current.locator("#connection")).toHaveText("connected");
    const cache = await current.locator("#terrain-cache").innerText();
    const resident = Number(/\((\d+) \/ 512 chunks\)/.exec(cache)?.[1]);
    expect(resident).toBeGreaterThan(0);
    expect(resident).toBeLessThanOrEqual(512);
    memorySamples.push({
      residentBytes: (await processMemory(memorySession)).residentBytes,
      camera: await current.locator("#world-position").innerText(),
      cache,
    });
  }
  await current.keyboard.press("Home");
  await expect(current.locator("#world-position")).toHaveText(origin);
  await current.waitForLoadState("networkidle");
  await memorySession.detach();
  const distinctCameras = new Set(memorySamples.map((sample) => sample.camera))
    .size;
  const residentBytes = memorySamples.map((sample) => sample.residentBytes);
  const maximumResidentBytes = Math.max(...residentBytes);
  const finalWindow = residentBytes.slice(-4);
  const finalSpread = Math.max(...finalWindow) - Math.min(...finalWindow);
  await writeFile(
    `${directory}/memory.json`,
    JSON.stringify(
      {
        content_hash: hash,
        unique_source_chunks: memoryChunks.size,
        eviction_limit_exercised: memoryChunks.size > 512,
        sampled_renderer_cameras: distinctCameras,
        maximum_sampled_chromium_rss_bytes: maximumResidentBytes,
        final_four_sample_spread_bytes: finalSpread,
        samples: memorySamples,
        limits:
          "24 fixed source-region drags with a single stationary primary; sampled summed Chromium VmRSS excludes GPU residency, moving armies and indefinite soak",
      },
      null,
      2,
    ),
  );
  // This 20k-tile candidate may not encounter enough distinct chunks to prove eviction.
  expect(memoryChunks.size).toBeGreaterThan(0);
  expect(distinctCameras).toBeGreaterThan(12);
  expect(rendererErrors).toEqual([]);
  expect(maximumResidentBytes).toBeLessThanOrEqual(1_073_741_824);
  expect(finalSpread).toBeLessThanOrEqual(67_108_864);
  await writeFile(
    `${directory}/browser.json`,
    JSON.stringify(
      {
        policy: "explicit-candidate-three-backend-source-capture-v1",
        content_hash: hash,
        observations,
        live_movement:
          "browser order is retained as raw versioned WebSocket frames and Rust-decoded by the disposable harness",
        limits:
          "isolated candidate activation only; local controller move and source screenshots, not eviction pressure, global traversal or dedicated hardware",
      },
      null,
      2,
    ),
  );
  if (current !== page) await current.close();
});
