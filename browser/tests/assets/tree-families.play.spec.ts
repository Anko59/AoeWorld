import { expect, test } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import { join } from "node:path";
import { PNG } from "pngjs";
import { forceCanvas, forceWebGl } from "../rendering/backends.js";

// Explicit private-art qualification. Public E2E does not turn absent originals
// into a pass; synthetic library pixels separately pin family/shadow selection.
test("original optional tree families render on all three backends", async ({
  browser,
  request,
}) => {
  test.skip(
    process.env["AOE_TREE_ART_PROOF"] !== "1",
    "Requires explicit private original-pack proof",
  );
  const output = process.env["AOE_TREE_ART_OUTPUT"];
  if (!output)
    throw new Error("AOE_TREE_ART_OUTPUT is required for private captures");
  const response = await request.get("/asset-pack/manifest.json");
  expect(response.ok()).toBeTruthy();
  const manifest = await response.json();
  for (const [id, count] of [
    [4654, 9],
    [4653, 13],
  ] as const) {
    const frames = manifest.frames.filter(
      (frame: { source: string }) =>
        frame.source === `graphics.drs:[32, 112, 108, 115]:${id}`,
    );
    expect(frames).toHaveLength(count);
  }
  await mkdir(output, { recursive: true });
  for (const backend of ["webgpu", "webgl2", "canvas2d"]) {
    const page = await browser.newPage({
      viewport: { width: 1280, height: 720 },
    });
    try {
      if (backend === "webgl2") await forceWebGl(page);
      if (backend === "canvas2d") await forceCanvas(page);
      const errors: string[] = [];
      page.on("pageerror", (error) => errors.push(error.message));
      await page.goto("/");
      await expect(page.locator("#connection")).toHaveText("connected", {
        timeout: 30_000,
      });
      await expect(page.locator("#playground")).toHaveAttribute(
        "data-assets",
        "aoe2-local",
      );
      await expect(page.locator("#playground")).toHaveAttribute(
        "data-renderer",
        backend,
      );
      await page
        .getByRole("button", { name: "Center on primary unit" })
        .click();
      const position = await page.locator("#world-position").innerText();
      const match = position.match(/(-?\d+(?:\.\d+)?),\s*(-?\d+(?:\.\d+)?)/);
      if (!match) throw new Error(`Unrecognized camera position: ${position}`);
      const x = Math.floor(Number(match[1]));
      const y = Math.floor(Number(match[2]));
      const chunk = treeChunk(x, y);
      const bind = async (value: ReturnType<typeof treeChunk>) => {
        await page.evaluate(
          async (json) => {
            const module = await import(
              new URL("/pkg/aoe_client.js", location.href).href
            );
            module.activate_surface_fixture("03".repeat(32), json);
          },
          JSON.stringify([value]),
        );
      };
      await bind(chunk);
      // Reset the source-map altitude focus to the newly bound flat fixture;
      // otherwise native bodies can be projected offscreen despite residency.
      await page
        .getByRole("button", { name: "Center on primary unit" })
        .click();
      await page.locator("#scene").hover();
      // Two animation frames ensure the newly bound scene is presented.
      await page.evaluate(
        () =>
          new Promise<void>((resolve) =>
            requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
          ),
      );
      const populated = PNG.sync.read(
        await page.locator("#scene").screenshot(),
      );
      await page.screenshot({ path: join(output, `${backend}-families.png`) });
      // Residency removal is not server depletion. The client overlay regression
      // separately proves amount-zero hides both presentations without resurrection.
      await bind(treeChunk(x, y, false));
      await page.evaluate(
        () =>
          new Promise<void>((resolve) =>
            requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
          ),
      );
      const removed = PNG.sync.read(await page.locator("#scene").screenshot());
      expect(removed.width).toBe(populated.width);
      expect(removed.height).toBe(populated.height);
      let changed = 0;
      for (let pixel = 0; pixel < populated.data.length; pixel += 4) {
        if (
          [0, 1, 2].some(
            (channel) =>
              Math.abs(
                (populated.data[pixel + channel] ?? 0) -
                  (removed.data[pixel + channel] ?? 0),
              ) > 20,
          )
        )
          changed++;
      }
      expect(
        changed,
        `${backend} native bodies must actually be visible before removal`,
      ).toBeGreaterThan(1000);
      await page.screenshot({ path: join(output, `${backend}-removed.png`) });
      expect(errors).toEqual([]);
    } finally {
      await page.close();
    }
  }
});

function treeChunk(x: number, y: number, populated = true) {
  const cx = Math.floor(x / 32);
  const cy = Math.floor(y / 32);
  // Keep the fixture near the generic camera and inside one chunk even when
  // its center is on a boundary. These are test inputs, not generator rules.
  const tx = cx * 32 + Math.min(27, Math.max(4, x - cx * 32));
  const ty = cy * 32 + Math.min(27, Math.max(4, y - cy * 32));
  const positions = populated
    ? [1, 2, 4].map((family, index) => [
        tx - 2 + index * 2,
        ty + 2 - index * 2,
        family,
      ])
    : [];
  const bytes = [3, 0, 4, positions.length, 0, 0, 0];
  const push = (value: number, count: number) => {
    for (let byte = 0; byte < count; byte++)
      bytes.push((value >>> (byte * 8)) & 255);
  };
  for (let ly = 0; ly < 32; ly++)
    for (let lx = 0; lx < 32; lx++) {
      push(0, 4);
      push(0, 2);
      for (let corner = 0; corner < 4; corner++) push(0, 2);
      push(1 << 20, 4);
      push(0, 2);
      push(cx * 32 + lx, 4);
      push(cy * 32 + ly, 4);
      for (let absent = 0; absent < 9; absent++) bytes.push(0);
    }
  positions.forEach(([tx = 0, ty = 0, family = 0], index) => {
    push((index + 1) * 2, 4);
    push(0, 4);
    push(tx, 4);
    push(ty, 4);
    bytes.push(1, 0, 100, 0, 0, family);
  });
  return {
    x: cx,
    y: cy,
    payload_hex: bytes
      .map((value) => value.toString(16).padStart(2, "0"))
      .join(""),
  };
}
