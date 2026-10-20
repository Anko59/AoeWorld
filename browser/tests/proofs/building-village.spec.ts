import { expect } from "@playwright/test";
import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { test } from "../../server.js";
import { forceWebGl } from "../rendering/backends.js";

// Optional local proof against this checkout's real server and licensed assets.
// Hash the bytes actually served, and require them to match this worktree.
test("capture this worktree's native DAT village and its build evidence", async ({
  page,
}) => {
  const config = await readFile(
    "../.cache/tc-shots/capture-request.json",
    "utf8",
  )
    .then(
      (text) =>
        JSON.parse(text) as {
          url: string;
          x: number;
          y: number;
          revision: string;
          fog?: boolean;
        },
    )
    .catch(() => null);
  test.skip(!config, "Local native village proof was not requested");
  if (!config) return;
  test.setTimeout(120_000);
  const origin = new URL(config.url);
  expect(["127.0.0.1", "localhost"]).toContain(origin.hostname);
  expect(["8080", "8081", ""]).not.toContain(origin.port);
  const healthResponse = await page.request.get(`${origin}health`);
  expect(healthResponse.ok()).toBe(true);
  const health = (await healthResponse.json()) as { build: string };
  expect(health.build).toBe(config.revision);
  const sha256 = (bytes: Buffer) =>
    createHash("sha256").update(bytes).digest("hex");
  const served = await page.request.get(`${origin}pkg/aoe_client_bg.wasm`);
  expect(served.ok()).toBe(true);
  const bundleHash = sha256(await served.body());
  expect(bundleHash).toBe(
    sha256(await readFile("../web/pkg/aoe_client_bg.wasm")),
  );
  const manifestResponse = await page.request.get(
    `${origin}asset-pack/manifest.json`,
  );
  const overlayResponse = await page.request.get(
    `${origin}asset-pack/buildings/manifest.json`,
  );
  expect(manifestResponse.ok()).toBe(true);
  expect(overlayResponse.ok()).toBe(true);
  const manifest = (await manifestResponse.json()) as { buildings: unknown[] };
  expect(manifest.buildings.length).toBeGreaterThan(0);
  await page.setViewportSize({ width: 1280, height: 720 });
  await forceWebGl(page);
  await page.goto(`${origin}?play=1#x=${config.x}&y=${config.y}&zoom=.7`);
  const scene = page.locator("#scene");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await expect(scene).toHaveAttribute("data-village-art", "native", {
    timeout: 60_000,
  });
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    "webgl2",
  );
  await expect(scene).toHaveAttribute("data-chunks-pending", "0", {
    timeout: 60_000,
  });
  await expect(scene).toHaveAttribute("data-chunks-failed", "0");
  await expect
    .poll(
      async () =>
        scene.evaluate((canvas) => {
          const actors = JSON.parse(
            canvas.getAttribute("data-actors") ?? "[]",
          ) as {
            kind: string;
            x: number;
            y: number;
          }[];
          return ["TownCenter", "House", "Mill", "Barracks", "Farm"].every(
            (kind) =>
              actors.some(
                (a) =>
                  a.kind === kind &&
                  a.x > 0 &&
                  a.x < 1280 &&
                  a.y > 0 &&
                  a.y < 600,
              ),
          );
        }),
      { timeout: 60_000 },
    )
    .toBe(true);
  const capture = async (name: string) => {
    const screenshot = await page.screenshot({
      path: `../.cache/tc-shots/${name}.png`,
    });
    const state = await scene.evaluate((canvas) => ({
      actors: JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as unknown,
      fogDisabled: canvas.getAttribute("data-fog-disabled"),
      fogVisible: canvas.getAttribute("data-fog-visible"),
      fogExplored: canvas.getAttribute("data-fog-explored"),
      atlasFrames: document
        .getElementById("playground")
        ?.getAttribute("data-atlas-frames"),
      renderer: document
        .getElementById("playground")
        ?.getAttribute("data-renderer"),
      map: document.getElementById("minimap")?.getAttribute("data-map-hash"),
      camera: ["x", "y"].map((axis) =>
        document.getElementById("minimap")?.getAttribute(`data-camera-${axis}`),
      ),
      zoom: document.getElementById("minimap")?.getAttribute("data-zoom"),
    }));
    await writeFile(
      `../.cache/tc-shots/${name}.json`,
      JSON.stringify(
        {
          origin: origin.href,
          health,
          bundleSha256: bundleHash,
          screenshotSha256: sha256(screenshot),
          manifestSha256: sha256(await manifestResponse.body()),
          overlaySha256: sha256(await overlayResponse.body()),
          ...state,
        },
        null,
        2,
      ),
    );
  };
  await capture("final-village");
  const camera = async (x: number, y: number, zoom: number) => {
    const previousFrame = Number(await scene.getAttribute("data-frame"));
    await page.evaluate(
      ({ x, y, zoom }) => {
        location.hash = `x=${x}&y=${y}&zoom=${zoom}`;
      },
      { x, y, zoom },
    );
    await expect(page.locator("#minimap")).toHaveAttribute(
      "data-zoom",
      String(zoom),
    );
    await expect(scene).toHaveAttribute("data-chunks-pending", "0");
    await expect(scene).toHaveAttribute("data-chunks-failed", "0");
    await expect
      .poll(async () => Number(await scene.getAttribute("data-frame")))
      .toBeGreaterThan(previousFrame);
  };
  await camera(config.x - 0.25, config.y - 0.25, 1.2);
  await capture("final-tc-close");
  if (config.fog) {
    await camera(config.x + 10, config.y + 3, 0.35);
    await expect(scene).toHaveAttribute("data-fog-disabled", "false");
    await expect
      .poll(
        async () =>
          Number(await scene.getAttribute("data-fog-explored")) -
          Number(await scene.getAttribute("data-fog-visible")),
      )
      .toBeGreaterThan(40);
    const memory = JSON.parse(
      await readFile("../.cache/tc-shots/final-fog-memory.json", "utf8"),
    ) as {
      building: { id: number };
    };
    await expect
      .poll(async () =>
        scene.evaluate((canvas, id) => {
          const actors = JSON.parse(
            canvas.getAttribute("data-actors") ?? "[]",
          ) as {
            id: number;
            kind: string;
            x: number;
            y: number;
          }[];
          return actors.some(
            (a) =>
              a.id === id &&
              a.kind === "TownCenter" &&
              a.x > 0 &&
              a.x < 1280 &&
              a.y > 0 &&
              a.y < 600,
          );
        }, memory.building.id),
      )
      .toBe(true);
    await capture("final-fog");
  }
});
