import { expect } from "@playwright/test";
import { test } from "../../server.js";

for (const resumed of [false, true]) {
  for (const unavailable of ["pending", "503", "404"]) {
    test(`${resumed ? "resumed" : "new"} match is playable while its minimap is ${unavailable}`, async ({
      page,
    }) => {
      await page.route("**/start/bootstrap.js", (route) =>
        route.fulfill({
          contentType: "application/javascript",
          body: `
          import { initInterface } from '/interface/hud.js';
          import { initNavigation } from '/navigation.js';
          import { initStartSequence } from '/start/sequence.js';
          initInterface(); initNavigation();
          window.aoeStart = initStartSequence({ readJson: r => r.json(), start() {}, cancel() {} });
        `,
        }),
      );
      if (resumed)
        await page.addInitScript(() =>
          localStorage.setItem("aoeworld.resume-token", "a".repeat(48)),
        );
      let requests = 0;
      await page.route("**/maps/*/minimap", (route) => {
        requests++;
        if (requests === 1) {
          if (unavailable === "pending") return;
          return route.fulfill({ status: Number(unavailable) });
        }
        return route.fulfill({
          contentType: "application/octet-stream",
          body: Buffer.from([65, 87, 77, 77, 2, 0, 0, 0, 0, 0, 0, 0]),
        });
      });
      await page.clock.install();
      await page.goto("/?skipIntro=1");
      if (!resumed)
        await page
          .getByRole("button", { name: "Single Player", exact: true })
          .click();
      await page.evaluate((resumed) => {
        const start = Reflect.get(window, "aoeStart");
        const nav = Reflect.get(window, "aoeNavigation");
        const scene = document.getElementById("scene");
        const connection = document.getElementById("connection");
        if (!scene || !connection) throw new Error("scene UI missing");
        scene.dataset["frame"] = "1";
        scene.dataset["viewPainted"] = "true";
        scene.dataset["chunksPending"] = scene.dataset["chunksFailed"] = "0";
        connection.textContent = "connected";
        if (resumed)
          start.resume({ phase: "ready", content_hash: "b".repeat(64) });
        else {
          start.begin();
          start.activated({
            start_available: true,
            content_hash: "b".repeat(64),
          });
        }
        // The renderer publishes markers after applying input and drawing.
        // frame() only consumes navigation input since the canvas sync split.
        nav.present(
          new Float64Array([256, 256, 64, 64, 64, 64, 1200, 800, 1, 0, 1]),
        );
        void nav.map(new Uint8Array(32).fill(187));
        scene.dataset["frame"] = "2";
      }, resumed);
      await expect.poll(() => requests).toBe(1);
      await page.clock.fastForward(1000);
      await expect(page.locator("#start-sequence")).toBeHidden({
        timeout: 1000,
      });
      await expect(page.locator("body")).toHaveAttribute(
        "data-start-state",
        "game",
      );
      if (unavailable === "pending") {
        await page.clock.fastForward(44_001);
        await expect(page.locator("#minimap")).toHaveAttribute(
          "data-raster",
          "failed",
        );
      }
      await page.clock.fastForward(5000);
      await expect.poll(() => requests).toBe(2);
      await expect(page.locator("#minimap")).toHaveAttribute(
        "data-raster",
        "ready",
      );
      await page.clock.fastForward(1000);
      await expect(page.locator("#start-sequence")).toBeHidden();
      await expect(page.locator("body")).toHaveAttribute(
        "data-start-state",
        "game",
      );
    });
  }
}

test("publishes the fine minimap without reading a GPU-backed canvas or blocking on a data URL", async ({
  page,
}) => {
  await page.addInitScript(() => {
    const probed = new WeakSet<HTMLCanvasElement>();
    const getContext = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = function (
      this: HTMLCanvasElement,
      type: string,
      options?: CanvasRenderingContext2DSettings,
    ) {
      if (type === "2d" && this.width === 32 && this.height === 32) {
        if (!options?.willReadFrequently)
          throw new Error("Minimap needs a CPU-backed raster");
        probed.add(this);
      }
      return Reflect.apply(getContext, this, [type, options]);
    } as typeof getContext;
    const toDataURL = HTMLCanvasElement.prototype.toDataURL;
    HTMLCanvasElement.prototype.toDataURL = function (...args) {
      if (probed.has(this)) throw new Error("Synchronous minimap encoding");
      return toDataURL.apply(this, args);
    };
  });
  await page.route("**/maps/*/minimap/coarse", (route) =>
    route.fulfill({ status: 503 }),
  );
  await page.route("**/maps/*/minimap", (route) =>
    route.fulfill({
      contentType: "application/octet-stream",
      body: Buffer.concat([
        Buffer.from([65, 87, 77, 77, 32, 0, 0, 0]),
        Buffer.alloc(32 * 32, 0x80),
      ]),
    }),
  );
  await page.goto("/?skipIntro=1");
  await page.evaluate(async () => {
    const nav = Reflect.get(window, "aoeNavigation");
    await nav.map(new Uint8Array(32).fill(187));
  });
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-raster",
    "ready",
  );
  const url = await page.locator("#minimap-terrain").getAttribute("href");
  expect(url).toMatch(/^blob:/);
  expect(
    await page.evaluate(async (url) => {
      const blob = await (await fetch(url ?? "")).blob();
      const image = await createImageBitmap(blob);
      const context = new OffscreenCanvas(32, 32).getContext("2d");
      if (!context) throw new Error("Missing pixel observer");
      context.drawImage(image, 0, 0);
      image.close();
      return Array.from(context.getImageData(0, 0, 1, 1).data);
    }, url),
  ).toEqual([124, 164, 82, 255]);

  await page.evaluate(async () => {
    const nav = Reflect.get(window, "aoeNavigation");
    await nav.map(new Uint8Array(32).fill(204));
  });
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-raster",
    "ready",
  );
  expect(await page.locator("#minimap-terrain").getAttribute("href")).not.toBe(
    url,
  );
  expect(
    await page.evaluate(async (url) => {
      try {
        await fetch(url ?? "");
        return false;
      } catch {
        return true;
      }
    }, url),
  ).toBe(true);
});
