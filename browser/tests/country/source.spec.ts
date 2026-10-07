import { expect, test, type Page } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";
import { PNG } from "pngjs";
import { forceCanvas, forceWebGl } from "../rendering/backends.js";

const hash = process.env["AOE_COUNTRY_SOURCE_HASH"];
const assetHash =
  "7e6fa0da194d13fcff0cd50e74fe92215fd11b5bcce4556e4dad3d34ff7447ce";
const directory = process.env["AOE_COUNTRY_SOURCE_OUTPUT"];
function outputDirectory() {
  if (!directory)
    throw new Error("missing current-run country source output directory");
  return directory;
}
test.skip(!hash, "explicit original-pack candidate qualification only");
test.setTimeout(540_000);

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
  expect(manifest.input_hash).toBe(assetHash);
  return manifest;
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
  await page.getByRole("button", { name: "Reconnect to server" }).click();
  await ready(page);
  await page.getByRole("button", { name: "Open map creator" }).click();
  const observations: unknown[] = [];
  let current = page;
  for (const backend of ["webgpu", "webgl2", "canvas2d"] as const) {
    const fetched = new Set<string>();
    const errors: string[] = [];
    current.on("response", (response) => {
      const path = new URL(response.url()).pathname;
      if (path.startsWith(`/maps/${hash}/chunks/`) && response.ok())
        fetched.add(path);
    });
    current.on("pageerror", (error) => errors.push(error.message));
    await current.goto("/");
    await ready(current);
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
      current = next;
    }
  }
  await writeFile(
    `${directory}/browser.json`,
    JSON.stringify(
      {
        policy: "explicit-candidate-three-backend-source-capture-v1",
        content_hash: hash,
        observations,
        limits:
          "isolated candidate activation only; source screenshots and bounded camera loading, not live move arrival, eviction pressure, global traversal or dedicated hardware",
      },
      null,
      2,
    ),
  );
  if (current !== page) await current.close();
});
