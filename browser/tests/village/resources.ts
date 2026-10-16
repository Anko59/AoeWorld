import { expect, type Page } from "@playwright/test";

export type Target = { kind: number; x: number; y: number; distance: number };
/** Observe one real HUD refresh; counters and actor coordinates share it. */
export async function freshHud(page: Page) {
  await page.evaluate(
    () =>
      new Promise<void>((resolve) => {
        const api = Reflect.get(window, "aoeInterface") as {
          numeric(values: Float64Array, status: string): void;
        };
        const original = api.numeric;
        api.numeric = (values, status) => {
          api.numeric = original;
          original.call(api, values, status);
          resolve();
        };
      }),
  );
}
export async function centerVillage(page: Page, select = false) {
  await page.keyboard.press("h");
  await freshHud(page);
  await expect
    .poll(async () =>
      page.locator("#scene").evaluate((canvas) => {
        const center = (
          JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as {
            kind: string;
            x: number;
            y: number;
          }[]
        ).find((actor) => actor.kind === "TownCenter");
        return center
          ? Math.abs(center.x - innerWidth / 2) +
              Math.abs(center.y - innerHeight / 2)
          : Infinity;
      }),
    )
    .toBeLessThan(1);
  if (!select) await page.keyboard.press("Escape");
  await page.locator("#scene").focus();
}
export async function resourceTargets(page: Page): Promise<Target[]> {
  return JSON.parse(
    (await page.locator("#scene").getAttribute("data-resource-targets")) ??
      "[]",
  ) as Target[];
}

/** Start with the Town Center centered; keep the real villager order selected. */
export async function frameResource(
  page: Page,
  kind: number,
  zoom = 0.35,
): Promise<Target> {
  await expect
    .poll(async () =>
      (await resourceTargets(page)).some((r) => r.kind === kind),
    )
    .toBe(true);
  const node = (await resourceTargets(page))
    .filter((r) => r.kind === kind)
    .sort((a, b) => a.distance - b.distance)[0];
  if (!node) throw new Error("Nearby resident resource missing");
  const camera = await page.locator("#minimap").evaluate((element) => ({
    x: Number(element.getAttribute("data-camera-x")),
    y: Number(element.getAttribute("data-camera-y")),
    zoom: Number(element.getAttribute("data-zoom")),
  }));
  const size = page.viewportSize();
  if (!size) throw new Error("Viewport missing");
  const dx = (node.x - size.width / 2) / (64 * camera.zoom);
  const dy = (node.y - size.height / 2) / (32 * camera.zoom);
  const worldX = camera.x + (dx + dy) / 2;
  const worldY = camera.y + (dy - dx) / 2;
  const x = (camera.x + worldX) / 2 + 2;
  const y = (camera.y + worldY) / 2 + 2;
  await page.evaluate(
    ([x, y, zoom]) => {
      location.hash = `x=${x}&y=${y}&zoom=${zoom}`;
    },
    [x, y, zoom],
  );
  const expectedX = size.width / 2 + (worldX - x - (worldY - y)) * 64 * zoom;
  const expectedY = size.height / 2 + (worldX - x + (worldY - y)) * 32 * zoom;
  await expect
    .poll(async () =>
      (await resourceTargets(page)).some(
        (r) =>
          r.kind === kind &&
          Math.abs(r.x - expectedX) < 2 &&
          Math.abs(r.y - expectedY) < 2,
      ),
    )
    .toBe(true);
  const target = (await resourceTargets(page)).find(
    (r) =>
      r.kind === kind &&
      Math.abs(r.x - expectedX) < 2 &&
      Math.abs(r.y - expectedY) < 2,
  );
  if (!target) throw new Error("Framed resource missing");
  return target;
}
