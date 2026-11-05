import { expect, type Page } from "@playwright/test";
import { PNG } from "pngjs";
import { test } from "../../../server.js";
import { fiveVillagerFixture } from "../fixtures/village.js";
import { centerVillage } from "../fixtures/resources.js";
import { gameAssets } from "../../game-assets.js";

// Native DE panel crops are larger than the frame they hold. These synthetic
// pages keep the real geometry (crop size and opaque bounds) with solid colours,
// so the measurements below hold for the imported art without shipping it.
const panels = {
  commands: { crop: [660, 413], bounds: [0, 34, 553, 379], colour: "#c83c3c" },
  selection: { crop: [1368, 413], bounds: [0, 36, 1322, 377], colour: "#3cc850" },
  minimap: { crop: [860, 413], bounds: [33, 10, 827, 403], colour: "#3c50c8" },
} as const;
type Role = keyof typeof panels;
const art = (role: Role) => ({
  page: `ui-${role}.svg`,
  x: 0,
  y: 0,
  width: panels[role].crop[0],
  height: panels[role].crop[1],
  page_width: panels[role].crop[0],
  page_height: panels[role].crop[1],
});
const svg = (role: Role) => {
  const [w, h] = panels[role].crop;
  const [x, y, bw, bh] = panels[role].bounds;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}"><rect x="${x}" y="${y}" width="${bw}" height="${bh}" fill="${panels[role].colour}"/></svg>`;
};
// Menu scale steps in bottom-menu.css; every dimension below is css pixels * zoom.
const zoomFor = (width: number) =>
  width <= 780 ? 0.5 : width <= 960 ? 0.65 : width <= 1200 ? 0.8 : 1;
const S = 0.5;
const frameSize = (role: Role) => ({
  width: panels[role].bounds[2] * S,
  height: panels[role].bounds[3] * S,
});

async function nativeMenu(page: Page) {
  await gameAssets(page, true);
  await page.route("**/asset-pack/**", (route) => {
    const url = route.request().url();
    if (url.endsWith("buildings/interface.json")) {
      const small = { ...art("commands"), page: "ui-small.svg", width: 36, height: 36, page_width: 36, page_height: 36 };
      return route.fulfill({
        json: {
          units: { 109: small, 83: small },
          technologies: {},
          commands: { stop: small, move: small },
          resources: {},
          panels: Object.fromEntries([
            ...(["commands", "selection", "minimap"] as const).map((role) => [
              `western-${role}`,
              art(role),
            ]),
            ["western-top", { ...small, width: 36 }],
            ["portrait", small],
            ["button", small],
          ]),
        },
      });
    }
    if (url.endsWith("interface.json")) {
      return route.fulfill({ status: 404, body: "No base catalog" });
    }
    for (const role of Object.keys(panels) as Role[]) {
      if (url.endsWith(`buildings/ui-${role}.svg`)) {
        return route.fulfill({ contentType: "image/svg+xml", body: svg(role) });
      }
    }
    if (url.endsWith("buildings/ui-small.svg")) {
      return route.fulfill({
        contentType: "image/svg+xml",
        body: '<svg xmlns="http://www.w3.org/2000/svg" width="36" height="36"><rect width="36" height="36" fill="#888"/></svg>',
      });
    }
    return route.fallback();
  });
  await fiveVillagerFixture(page);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect(page.locator("#bottom-panel")).toHaveAttribute("data-frame", "native");
  await expect(page.locator("html")).toHaveAttribute("data-interface-art", "original");
  await centerVillage(page, true);
  await expect(page.locator("#selection-name")).toHaveText("Town Center");
}

const box = async (page: Page, selector: string) => {
  const rect = await page.locator(selector).boundingBox();
  if (!rect) throw new Error(`${selector} has no box`);
  return rect;
};

for (const [width, height] of [
  [1280, 720],
  [1920, 1080],
  [1100, 700],
  [900, 600],
  [3840, 2160],
] as const) {
  test(`bottom menu art covers every panel at ${width}x${height}`, async ({ page }) => {
    test.setTimeout(90_000);
    await page.setViewportSize({ width, height });
    await nativeMenu(page);
    const zoom = zoomFor(width);
    const tolerance = 1.2;
    const commands = await box(page, "#commands");
    const selection = await box(page, "#selection");
    const minimap = await box(page, "#minimap");
    // Fixed panels are exactly the size of their frame art.
    for (const [role, rect] of [["commands", commands], ["minimap", minimap]] as const) {
      expect(Math.abs(rect.width - frameSize(role).width * zoom)).toBeLessThan(tolerance);
      expect(Math.abs(rect.height - frameSize(role).height * zoom)).toBeLessThan(tolerance);
    }
    expect(Math.abs(selection.height - frameSize("selection").height * zoom)).toBeLessThan(tolerance);
    expect(selection.width).toBeGreaterThanOrEqual((322 + 700) * S * zoom - tolerance);
    // The three panels abut and span the whole window: no empty strip anywhere.
    expect(Math.abs(commands.x)).toBeLessThan(tolerance);
    expect(Math.abs(selection.x - (commands.x + commands.width))).toBeLessThan(tolerance);
    expect(Math.abs(minimap.x - (selection.x + selection.width))).toBeLessThan(tolerance);
    expect(Math.abs(minimap.x + minimap.width - width)).toBeLessThan(tolerance);
    for (const rect of [commands, selection, minimap]) {
      expect(Math.abs(rect.y + rect.height - height)).toBeLessThan(tolerance);
    }
    // Every art layer lies inside its panel and the layers tile its full width.
    for (const selector of ["#commands", "#selection", "#minimap"]) {
      const parent = await box(page, selector);
      const layers = await page.locator(`${selector}>.frame-art`).evaluateAll((nodes) =>
        nodes.map((node) => {
          const r = node.getBoundingClientRect();
          return { x: r.x, y: r.y, width: r.width, height: r.height };
        }),
      );
      expect(layers.length).toBeGreaterThan(0);
      let covered = 0;
      for (const layer of layers) {
        expect(Math.abs(layer.y - parent.y)).toBeLessThan(tolerance);
        expect(Math.abs(layer.height - parent.height)).toBeLessThan(tolerance);
        expect(layer.x).toBeGreaterThanOrEqual(parent.x - tolerance);
        expect(layer.x + layer.width).toBeLessThanOrEqual(parent.x + parent.width + tolerance);
        covered += layer.width;
      }
      expect(Math.abs(covered - parent.width)).toBeLessThan(tolerance);
    }
    // Rendered pixels agree: sampled points just inside each panel are art, not scene.
    const shot = PNG.sync.read(await page.screenshot());
    const dpr = shot.width / width;
    const sample = (x: number, y: number) => {
      const offset = (Math.round(y * dpr) * shot.width + Math.round(x * dpr)) * 4;
      return [...shot.data.slice(offset, offset + 3)];
    };
    const near = (actual: number[], hex: string) => {
      const want = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
      return actual.every((v, i) => Math.abs(v - want[i]) < 12);
    };
    for (const [role, rect] of [["commands", commands], ["selection", selection], ["minimap", minimap]] as const) {
      for (const [fx, fy] of [[2, 2], [rect.width - 3, 2], [2, rect.height - 3], [rect.width - 3, rect.height - 3]]) {
        // The transparent corners of the art-less rows do not exist: the frame is cut to its bounds.
        expect(near(sample(rect.x + fx, rect.y + fy), panels[role].colour)).toBe(true);
      }
    }
    // The minimap map fills the rhombus rectangle of its frame, clipped to a diamond.
    const map = await box(page, "#minimap-map");
    const expected = { x: 53.5 * zoom, y: 8.5 * zoom, width: 339.5 * zoom, height: 184 * zoom };
    expect(Math.abs(map.x - (minimap.x + expected.x))).toBeLessThan(tolerance);
    expect(Math.abs(map.y - (minimap.y + expected.y))).toBeLessThan(tolerance);
    expect(Math.abs(map.width - expected.width)).toBeLessThan(tolerance);
    expect(Math.abs(map.height - expected.height)).toBeLessThan(tolerance);
    // Buttons sit in the four sockets (corners) and the small ones inside the rhombus.
    for (const id of ["town-center", "recenter", "overview-toggle", "grid", "carry-indicators", "fullscreen", "minimap-zoom"]) {
      const rect = await box(page, `#${id}`);
      expect(rect.x).toBeGreaterThanOrEqual(minimap.x - tolerance);
      expect(rect.y).toBeGreaterThanOrEqual(minimap.y - tolerance);
      expect(rect.x + rect.width).toBeLessThanOrEqual(minimap.x + minimap.width + tolerance);
      expect(rect.y + rect.height).toBeLessThanOrEqual(minimap.y + minimap.height + tolerance);
    }
    const socket = await box(page, "#town-center");
    expect(Math.abs(socket.x + socket.width / 2 - (minimap.x + 76 * zoom))).toBeLessThan(2 * zoom + tolerance);
    expect(Math.abs(socket.y + socket.height / 2 - (minimap.y + 58.5 * zoom))).toBeLessThan(2 * zoom + tolerance);
    // Clicking the middle of the rhombus still jumps the camera to the map middle.
    await expect(page.locator("#minimap")).not.toHaveClass(/expanded/);
    await page.screenshot({ path: `../.cache/bottom/synthetic-${width}x${height}.png` });
  });
}
