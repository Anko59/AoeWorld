import { expect, type Page } from "@playwright/test";
import { fiveVillagerFixture } from "./village.js";
import { centerVillage } from "./resources.js";
import { gameAssets } from "../../game-assets.js";
import { activateSyntheticMap } from "../../synthetic-map.js";
import { ownersFixture } from "../../rendering/fixtures/owners.js";

// Native DE panel crops are larger than the frame they hold. These synthetic
// pages keep the real geometry (crop size and opaque bounds) with solid colours,
// so the measurements below hold for the imported art without shipping it.
export const panels = {
  commands: { crop: [660, 413], bounds: [0, 34, 553, 379], colour: "#c83c3c" },
  selection: {
    crop: [1368, 413],
    bounds: [0, 36, 1322, 377],
    colour: "#3cc850",
  },
  minimap: { crop: [860, 413], bounds: [33, 10, 827, 403], colour: "#3c50c8" },
} as const;
export type Role = keyof typeof panels;
export const art = (role: Role) => ({
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
export const zoomFor = (width: number) =>
  width <= 780 ? 0.5 : width <= 960 ? 0.65 : width <= 1200 ? 0.8 : 1;
export const S = 0.5;
export const frameSize = (role: Role) => ({
  width: panels[role].bounds[2] * S,
  height: panels[role].bounds[3] * S,
});

export async function nativeMenu(page: Page, foreign = false) {
  await gameAssets(page, true);
  await page.route("**/asset-pack/**", (route) => {
    const url = route.request().url();
    if (url.endsWith("buildings/interface.json")) {
      const small = {
        ...art("commands"),
        page: "ui-small.svg",
        width: 36,
        height: 36,
        page_width: 36,
        page_height: 36,
      };
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
  // The foreign fixture holds one villager per player id; player 0 is local.
  if (foreign) await ownersFixture(page);
  else await fiveVillagerFixture(page);
  await page.goto("/?play=1#x=8192.5&y=8192.5&zoom=1");
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect(page.locator("#bottom-panel")).toHaveAttribute(
    "data-frame",
    "native",
  );
  await expect(page.locator("html")).toHaveAttribute(
    "data-interface-art",
    "original",
  );
  if (foreign) return activateSyntheticMap(page, () => false);
  await centerVillage(page, true);
  await expect(page.locator("#selection-name")).toHaveText(/\S/);
}

export const box = async (page: Page, selector: string) => {
  const rect = await page.locator(selector).boundingBox();
  if (!rect) throw new Error(`${selector} has no box`);
  return rect;
};

