import { expect, type Page } from "@playwright/test";
import { gameAssets } from "../../game-assets.js";
import { postcardFields } from "../../wire/combat.js";
import { fiveVillagerFixture } from "../fixtures/village.js";

export type Point = { x: number; y: number };
export type Order = {
  tag: number;
  ids: number[];
  // Formation and Some, formation, rally target or General command.
  extra: number[];
  to: Point;
  flags: number[];
};

// Client message variants carrying a counted list of members.
const groups = new Set([29, 41, 46]);
// Fields between the members and the destination.
const between = new Map([
  [20, 1],
  [29, 2],
  [32, 1],
  [41, 1],
]);
const tiles = (field = NaN) =>
  (field % 2 ? -(field + 1) / 2 : field / 2) / 1024;

/** A movement or rally order; every field of these messages is a varint. */
export function decode(bytes: Buffer): Order {
  const fields = postcardFields(bytes, bytes.length).filter(
    (field) => field >= 0,
  );
  const tag = fields[0] ?? -1;
  const first = groups.has(tag) ? 3 : 2;
  const last = first + (groups.has(tag) ? (fields[2] ?? 0) : 1);
  const at = last + (between.get(tag) ?? 0);
  return {
    tag,
    ids: fields.slice(first, last).sort((a, b) => a - b),
    extra: fields.slice(last, at),
    to: { x: tiles(fields[at]), y: tiles(fields[at + 1]) },
    flags: fields.slice(at + 2),
  };
}

/** Owned units are selected; every order the client sends is recorded. */
export async function army(
  page: Page,
  kind = "LightCavalry",
  unitKind = 13,
  group = true,
): Promise<Order[]> {
  const orders: Order[] = [];
  await gameAssets(page, true);
  await fiveVillagerFixture(
    page,
    (bytes) => {
      // Hello, Subscribe and Resync are not orders.
      if (![0, 1, 3].includes(bytes[0] ?? -1)) orders.push(decode(bytes));
    },
    () => 0,
    false,
    true,
    unitKind,
  );
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
  const scene = page.locator("#scene");
  await expect.poll(() => scene.getAttribute("data-actors")).toContain(kind);
  const actor = await scene.evaluate(
    (canvas, kind) =>
      (
        JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as {
          kind: string;
          x: number;
          y: number;
        }[]
      ).find((actor) => actor.kind === kind),
    kind,
  );
  if (!actor) throw new Error(`${kind} fixture missing`);
  if (group) await page.mouse.dblclick(actor.x, actor.y);
  else await page.mouse.click(actor.x, actor.y);
  await expect(page.locator("#selection")).toHaveAttribute(
    "data-count",
    group ? "5" : "1",
  );
  return orders;
}

/** Arms an order from its command button without moving the pointer. */
export async function armOrder(page: Page, id: string, mode: string) {
  await page
    .locator(id)
    .evaluate((button) => (button as HTMLButtonElement).click());
  await expect(page.locator("#scene")).toHaveAttribute("data-order-mode", mode);
}

/** The main camera and the minimap's own zoom and pan. */
export const camera = (page: Page) =>
  page
    .locator("#minimap")
    .evaluate((panel) =>
      [
        panel.dataset.cameraX,
        panel.dataset.cameraY,
        panel.dataset.zoom,
        document.getElementById("minimap-view")?.getAttribute("transform"),
      ].join(" "),
    );

/** The native menu's stretched 2:1 rhombus, without its private art. */
export async function nativeFrame(page: Page) {
  await page.evaluate(() => {
    for (const [id, width, height] of [
      ["commands", 276.5, 189.5],
      ["selection", 661, 188.5],
      ["minimap", 413.5, 201.5],
    ] as const) {
      const style = document.getElementById(id)?.style;
      style?.setProperty("--frame-width", `${width}px`);
      style?.setProperty("--frame-height", `${height}px`);
    }
    document
      .getElementById("bottom-panel")
      ?.setAttribute("data-frame", "native");
    document
      .getElementById("minimap-map")
      ?.setAttribute("preserveAspectRatio", "none");
  });
  await expect
    .poll(async () => (await mapBox(page)).width)
    .toBeCloseTo(339.5, 0);
}

export async function mapBox(page: Page) {
  const box = await page.locator("#minimap-map").boundingBox();
  if (!box) throw new Error("Minimap missing");
  return box;
}

/** Where the drawn map shows a world tile. The rendered frame is the oracle,
 * so zoom, pan, stretch and letterbox all count. */
export const shown = (page: Page, world: Point): Promise<Point> =>
  page.evaluate(({ x, y }) => {
    const frame = document.querySelector<SVGGElement>("#minimap-frame");
    const panel = document.querySelector<HTMLElement>("#minimap");
    const matrix = frame?.getScreenCTM();
    if (!panel || !matrix) throw new Error("Minimap missing");
    const point = new DOMPoint(
      (x / Number(panel.dataset.worldWidth)) * 256,
      (y / Number(panel.dataset.worldHeight)) * 256,
    ).matrixTransform(matrix);
    return { x: point.x, y: point.y };
  }, world);

/** The whole pixel nearest to a world tile. */
export async function pixel(page: Page, world: Point): Promise<Point> {
  const { x, y } = await shown(page, world);
  return { x: Math.round(x), y: Math.round(y) };
}

/** Ground orders snap to a tile centre, drawn under the clicked pixel. */
export async function expectAt(
  page: Page,
  order: Order | undefined,
  click: Point,
) {
  if (!order) throw new Error("No order was sent");
  expect([order.to.x % 1, order.to.y % 1]).toEqual([0.5, 0.5]);
  const { x, y } = await shown(page, order.to);
  expect(Math.hypot(x - click.x, y - click.y)).toBeLessThan(1);
}

/** Points of the map's box with no world under them: the corners, a little
 * beyond each border, and the letterbox band of the square map. */
export async function outside(
  page: Page,
  stretched: boolean,
): Promise<[Point, ...Point[]]> {
  const box = await mapBox(page);
  const side = Math.min(box.width, box.height);
  const w = stretched ? box.width : side;
  const h = stretched ? box.height : side;
  const at = (u: number, v: number, dx = 0, dy = 0) => ({
    x: Math.round(box.x + box.width / 2 + u * w) + dx,
    y: Math.round(box.y + box.height / 2 + v * h) + dy,
  });
  return [
    at(-0.45, 0.45),
    at(-0.45, -0.45),
    at(0.45, -0.45),
    at(0.45, 0.45),
    at(0.25, -0.25, 4, -4),
    at(0.25, 0.25, 4, 4),
    at(-0.25, 0.25, -4, 4),
    at(-0.25, -0.25, -4, -4),
    ...(box.width - w > 16
      ? [{ x: Math.round(box.x + 3), y: at(0, 0).y }]
      : []),
  ];
}

/** Right presses delivered in one task, past the hit test of a clip path. */
export const press = (
  page: Page,
  points: Point[],
  keys: { altKey?: boolean; ctrlKey?: boolean } = {},
) =>
  page.evaluate(
    ({ points, keys }) => {
      for (const { x, y } of points)
        document.getElementById("minimap-map")?.dispatchEvent(
          new PointerEvent("pointerdown", {
            bubbles: true,
            cancelable: true,
            isPrimary: true,
            pointerType: "mouse",
            button: 2,
            buttons: 2,
            clientX: x,
            clientY: y,
            ...keys,
          }),
        );
    },
    { points, keys },
  );

export type Ping = { kind: string | null; x: number; y: number };
/** Keeps the middle of every minimap ping from the moment it is drawn. */
export const recordPings = (page: Page) =>
  page.evaluate(() => {
    const pings = new Map<Element, unknown>();
    Reflect.set(window, "minimapPings", pings);
    new MutationObserver(() => {
      for (const node of document.querySelectorAll("[data-minimap-ping]")) {
        const box = node.getBoundingClientRect();
        pings.delete(node);
        pings.set(node, {
          kind: node.getAttribute("data-minimap-ping"),
          x: box.x + box.width / 2,
          y: box.y + box.height / 2,
        });
      }
    }).observe(document.body, {
      subtree: true,
      childList: true,
      attributes: true,
      attributeFilter: ["data-minimap-ping"],
    });
  });

export const lastPing = (page: Page): Promise<Ping | undefined> =>
  page.evaluate(() =>
    [
      ...(Reflect.get(window, "minimapPings") as Map<Element, Ping>).values(),
    ].at(-1),
  );
