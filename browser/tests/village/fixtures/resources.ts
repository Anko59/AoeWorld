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

/** Scout the synthetic forest before resource tests select its remembered nodes. */
export async function scoutVillage(page: Page, dx: number, dy: number) {
  await centerVillage(page);
  const scout = (
    JSON.parse(
      (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
    ) as {
      kind: string;
      x: number;
      y: number;
    }[]
  ).find((actor) => actor.kind === "Scout");
  if (!scout) throw new Error("Scout missing");
  await page.mouse.click(scout.x, scout.y);
  await expect(page.locator("#selection-name")).toContainText("Scout");
  const camera = await page.locator("#minimap").evaluate((el) => ({
    x: Number(el.getAttribute("data-camera-x")),
    y: Number(el.getAttribute("data-camera-y")),
  }));
  await page.evaluate(
    ({ x, y, dx, dy }) => {
      location.hash = `x=${x + dx}&y=${y + dy}&zoom=.5`;
    },
    { ...camera, dx, dy },
  );
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-camera-x",
    String(camera.x + dx),
  );
  const size = page.viewportSize();
  if (!size) throw new Error("Viewport missing");
  await freshHud(page);
  await page.mouse.click(size.width / 2, size.height / 2, { button: "right" });
  await expect(page.locator("#unit-state")).not.toHaveText("unit idle");
  await expect(page.locator("#unit-state")).toHaveText("unit idle", {
    timeout: 25_000,
  });
  await centerVillage(page);
}

/** Start with the Town Center centered; keep the real villager order selected. */
export async function frameResource(
  page: Page,
  kind: number,
  zoom = 0.35,
  centered = false,
): Promise<Target> {
  if (centered) return centerVisibleResource(page, kind, zoom);
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

/** Follow the displayed terrain projection, including changing focus height.
 * Flat-world inversion alone cannot frame an elevated deposit reliably. */
async function centerVisibleResource(
  page: Page,
  kind: number,
  zoom: number,
): Promise<Target> {
  const size = page.viewportSize();
  if (!size) throw new Error("Viewport missing");
  for (let attempt = 0; attempt < 8; attempt++) {
    await freshHud(page);
    const node = (await resourceTargets(page)).find(
      (node) => node.kind === kind,
    );
    if (!node) throw new Error("Resident resource missing");
    if (
      node.x > 40 &&
      node.x < size.width - 40 &&
      node.y > 80 &&
      node.y < size.height - 240
    )
      return node;
    const camera = await page.locator("#minimap").evaluate((element) => ({
      x: Number(element.dataset["cameraX"]),
      y: Number(element.dataset["cameraY"]),
      zoom: Number(element.dataset["zoom"]),
    }));
    const dx = (node.x - size.width / 2) / (64 * camera.zoom);
    const dy = (node.y - size.height / 2) / (32 * camera.zoom);
    const x = camera.x + (dx + dy) / 2,
      y = camera.y + (dy - dx) / 2;
    await page.evaluate(
      ([x, y, zoom]) => {
        location.hash = `x=${x}&y=${y}&zoom=${zoom}`;
      },
      [x, y, zoom],
    );
    await expect
      .poll(async () =>
        Number(await page.locator("#minimap").getAttribute("data-camera-x")),
      )
      .toBeCloseTo(x);
  }
  throw new Error(
    "The requested resource did not enter the visible battlefield",
  );
}
