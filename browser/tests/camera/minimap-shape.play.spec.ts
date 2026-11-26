import { expect, type Page } from "@playwright/test";
import { test } from "../../server.js";

type Point = [number, number];
type Frame = { width: number; height: number; stretch: boolean };
/** `polygon`: the four unclamped ground corners, clockwise from top left. */
type Input = Frame & {
  polygon: Point[];
  world: Point;
  view: { zoom: number; center: Point };
};
/** CSS px from the top left of the drawn map. */
type Shape = {
  kind: "spot" | "inset" | "frame" | "cover" | "arrow";
  /** The locator brackets: never smaller than the minimum box. */
  box: { x: number; y: number; width: number; height: number };
  /** The true view, projected and cut (not clamped) at the world's border; a
   * frame also at the visible map's. The inner border itself for a cover. */
  polygon: Point[];
  /** Which brackets are drawn, in the order of the input polygon. */
  corners: boolean[];
  /** Outline of the arrowhead, its tip first. */
  arrow: Point[] | null;
};

const native: Frame = { width: 339.5, height: 184, stretch: true };
const square: Frame = { width: 139, height: 139, stretch: false };
const overview: Frame = { width: 860, height: 860, stretch: false };
const whole = (side: number) => ({
  world: [side, side] as Point,
  view: { zoom: 1, center: [side / 2, side / 2] as Point },
});

/** The ground a screen shows, as navigation.js present() derives it. */
function ground([x, y]: Point, zoom = 1, screen: Point = [1728, 1000]) {
  return (
    [
      [-1, -1],
      [1, -1],
      [1, 1],
      [-1, 1],
    ] as const
  ).map(([across, down]): Point => {
    const dx = (across * screen[0]) / (256 * zoom);
    const dy = (down * screen[1]) / (128 * zoom);
    return [x + dx + dy, y - dx + dy];
  });
}
function project(
  [x, y]: Point,
  { world, view, width, height }: Omit<Input, "polygon">,
): Point {
  const dx = ((x - view.center[0]) / world[0]) * view.zoom;
  const dy = ((y - view.center[1]) / world[1]) * view.zoom;
  return [(0.5 + (dx - dy) / 2) * width, (0.5 + (dx + dy) / 2) * height];
}
function unproject(
  [px, py]: Point,
  { world, view, width, height }: Omit<Input, "polygon">,
): Point {
  const across = (px / width - 0.5) / view.zoom;
  const down = (py / height - 0.5) / view.zoom;
  return [
    view.center[0] + (across + down) * world[0],
    view.center[1] + (down - across) * world[1],
  ];
}
/** 1 on the border of the diamond (or square), less inside it. */
function reach([px, py]: Point, width: number, height: number, diamond = true) {
  const across = Math.abs(px / width - 0.5) * 2;
  const down = Math.abs(py / height - 0.5) * 2;
  return diamond ? across + down : Math.max(across, down);
}
function middle(box: Shape["box"]): Point {
  return [box.x + box.width / 2, box.y + box.height / 2];
}
function distance([ax, ay]: Point, [bx, by]: Point) {
  return Math.hypot(ax - bx, ay - by);
}
function samePoints(actual: Point[], expected: Point[], within = 1e-6) {
  const apart = (from: Point[], to: Point[]) =>
    from.map((a) => Math.min(...to.map((b) => distance(a, b))));
  expect(Math.max(...apart(expected, actual))).toBeLessThan(within);
  expect(Math.max(...apart(actual, expected))).toBeLessThan(within);
}
/** A view of 16,384 tiles at camera zoom .02, 3.7 times as wide as the minimap
 * zoomed 90 times, with its top right corner at that px of the map. */
function wideView(map: Frame, right: number, top: number, centre?: Point) {
  const center: Point = centre ?? [8192, 8192];
  const world: Point = [16384, 16384];
  const base = { ...map, world, view: { zoom: 90, center } };
  const scale = 90 / (0.02 * 16384 * 256);
  const camera = unproject(
    [right - 1728 * scale * map.width, top + 2000 * scale * map.height],
    base,
  );
  return { ...base, polygon: ground(camera, 0.02) };
}

async function shapes<T extends Input>(page: Page, inputs: T[]) {
  await page.goto("/diagnostics.html");
  const plain: Input[] = inputs;
  const found = await page.evaluate(async (inputs): Promise<Shape[]> => {
    const { markerShape } = await import("/navigation/marker.js" as string);
    return inputs.map((input) => markerShape(input));
  }, plain);
  return inputs.map((input, index) => {
    const shape = found[index];
    if (!shape) throw new Error("Shape missing");
    return { input, shape };
  });
}

test("the locator box follows the size of the map, never the minimap zoom", async ({
  page,
}) => {
  const camera: Point = [9000.25, 8000.75];
  const inputs = (
    [
      [339.5, 184, 30, 20],
      [271.6, 147.2, 28, 20],
      [139, 139, 28, 20],
      [90, 90, 26, 18],
      [860, 860, 48, 32],
      [2000, 2000, 48, 32],
    ] as const
  ).flatMap(([width, height, wide, high]) =>
    [1, 64].map((zoom) => ({
      polygon: ground(camera, 3, [1280, 720]),
      world: [16384, 16384] as Point,
      view: { zoom, center: zoom === 1 ? ([8192, 8192] as Point) : camera },
      width,
      height,
      stretch: width !== height,
      wide,
      high,
    })),
  );
  for (const { input, shape } of await shapes(page, inputs)) {
    expect([shape.box.width, shape.box.height]).toEqual([
      input.wide,
      input.high,
    ]);
    const [x, y] = project(camera, input);
    const [boxX, boxY] = middle(shape.box);
    expect(Math.abs(boxX - x)).toBeLessThan(0.501);
    expect(Math.abs(boxY - y)).toBeLessThan(0.501);
  }
});

test("the true view is exact from 4 px and takes over at the locator box", async ({
  page,
}) => {
  const camera: Point = [1050, 900];
  const inputs = (
    [
      [3.9, 3.9, "spot", 30, 20],
      [3.9, 12, "spot", 30, 20],
      [12, 3.9, "spot", 30, 20],
      [4.1, 4.1, "inset", 30, 20],
      [29.9, 19.9, "inset", 30, 20],
      [29.9, 40, "inset", 30, 40],
      [40, 19.9, "inset", 40, 20],
      [30.1, 20.1, "frame", 30.1, 20.1],
      [61.3, 33.7, "frame", 61.3, 33.7],
    ] as const
  ).map(([across, down, kind, wide, high]) => ({
    ...native,
    ...whole(2000),
    polygon: ground(camera, 1, [
      (across * 128 * 2000) / native.width,
      (down * 64 * 2000) / native.height,
    ]),
    kind,
    wide,
    high,
  }));
  for (const { input, shape } of await shapes(page, inputs)) {
    expect([input.wide, input.high, shape.kind]).toEqual([
      input.wide,
      input.high,
      input.kind,
    ]);
    expect(shape.box.width).toBeCloseTo(input.wide, 6);
    expect(shape.box.height).toBeCloseTo(input.high, 6);
    expect(shape.corners).toEqual([true, true, true, true]);
    if (shape.kind !== "spot")
      samePoints(
        shape.polygon,
        input.polygon.map((point) => project(point, input)),
      );
    expect(distance(middle(shape.box), project(camera, input))).toBeLessThan(
      shape.kind === "frame" ? 1e-6 : 0.71,
    );
  }
});

test("a camera on a border or a corner of the world keeps its whole locator", async ({
  page,
}) => {
  const inputs = [2000, 16384].flatMap((side) =>
    (
      [
        [native, 30, 20],
        [square, 28, 20],
        [overview, 48, 32],
      ] as const
    ).flatMap(([map, wide, high]) =>
      [0, 1 / 3, 0.617, 1].flatMap((part) =>
        (
          [
            [part, 0],
            [1, part],
            [1 - part, 1],
            [0, 1 - part],
          ] as const
        ).map(([x, y]) => ({
          ...map,
          ...whole(side),
          polygon: ground([x * side, y * side]),
          camera: [x * side, y * side] as Point,
          wide,
          high,
        })),
      ),
    ),
  );
  for (const { input, shape } of await shapes(page, inputs)) {
    expect([input.camera, ["spot", "inset"].includes(shape.kind)]).toEqual([
      input.camera,
      true,
    ]);
    expect([shape.box.width, shape.box.height]).toEqual([
      input.wide,
      input.high,
    ]);
    expect(shape.corners).toEqual([true, true, true, true]);
    const [x, y] = project(input.camera, input);
    const [boxX, boxY] = middle(shape.box);
    expect(Math.abs(boxX - x)).toBeLessThan(0.501);
    expect(Math.abs(boxY - y)).toBeLessThan(0.501);
    expect(shape.polygon.length).toBeGreaterThan(2);
    for (const point of shape.polygon)
      expect(reach(point, input.width, input.height)).toBeLessThan(1 + 1e-6);
  }
});

test("a large view is cut at the border of the world and drops the brackets beyond it", async ({
  page,
}) => {
  const base = { ...native, ...whole(2000) };
  const [east, around] = await shapes(page, [
    { ...base, polygon: ground([1960, 900], 0.05) },
    { ...base, polygon: ground([1050, 900], 0.01) },
  ]);
  if (!east || !around) throw new Error("Shape missing");
  for (const { shape } of [east, around]) expect(shape.kind).toBe("frame");
  expect(east.shape.corners).toEqual([true, true, false, true]);
  samePoints(
    east.shape.polygon,
    (
      [
        [1668.75, 878.75],
        [1938.75, 608.75],
        [2000, 670],
        [2000, 1172.5],
        [1981.25, 1191.25],
      ] as Point[]
    ).map((point) => project(point, base)),
  );
  expect(around.shape.corners).toEqual([false, false, false, false]);
  expect(around.shape.polygon.length).toBeGreaterThanOrEqual(8);
  for (const point of around.shape.polygon)
    expect(reach(point, base.width, base.height)).toBeLessThan(1 + 1e-6);
});

test("a frame is also cut at the visible minimap and lives while any part shows", async ({
  page,
}) => {
  const [band, corner, boxed, border, sliver, gone] = await shapes(page, [
    wideView(native, 199.75, -300),
    wideView(native, 199.75, 72),
    wideView(square, 100, 40),
    // The world ends on the diagonal of this square, below it is land.
    wideView(square, 100, 40, [8192, 0]),
    wideView(native, 5, -300),
    wideView(native, -5, -300),
  ]);
  if (!band || !corner || !boxed || !border || !sliver || !gone)
    throw new Error("Shape missing");
  // Where the right edge, 30 px from the middle, leaves the native diamond.
  const high = 92 * (1 - 30 / 169.75);
  const framed = (shape: Shape, topRight: boolean, polygon: Point[]) => {
    expect(shape.kind).toBe("frame");
    expect(shape.corners).toEqual([false, topRight, false, false]);
    samePoints(shape.polygon, polygon, 0.01);
  };
  framed(band.shape, false, [
    [0, 92],
    [169.75, 0],
    [199.75, 92 - high],
    [199.75, 92 + high],
    [169.75, 184],
  ]);
  framed(corner.shape, true, [
    [0, 92],
    [(169.75 * 20) / 92, 72],
    [199.75, 72],
    [199.75, 92 + high],
    [169.75, 184],
  ]);
  framed(boxed.shape, true, [
    [0, 40],
    [100, 40],
    [100, 139],
    [0, 139],
  ]);
  framed(border.shape, false, [
    [0, 40],
    [40, 40],
    [100, 100],
    [100, 139],
    [0, 139],
  ]);
  expect(sliver.shape.kind).toBe("frame");
  expect(gone.shape.kind).toBe("arrow");
});

test("a view covering the whole visible minimap is an outline inside its border", async ({
  page,
}) => {
  const world = (side: number): Point => [side, side];
  const half = (side: number): Point => [side / 2, side / 2];
  const covering = [native, square, overview].flatMap((map) =>
    (
      [
        [16384, 0.02, 90],
        [16384, 0.25, 1024],
        [2000, 1, 1024],
        [2000, 1 / 4096, 1],
      ] as const
    ).map(([side, zoom, minimap]) => ({
      ...map,
      world: world(side),
      view: { zoom: minimap, center: half(side) },
      polygon: ground(half(side), zoom),
      kind: "cover",
    })),
  );
  // A couple of px on either side of the border of the map.
  const edges = (
    [
      [native, 345, 190, "cover"],
      [native, 335, 190, "frame"],
      [native, 345, 180, "frame"],
      [square, 141, 141, "cover"],
      [square, 137, 141, "frame"],
      [square, 141, 137, "frame"],
    ] as const
  ).map(([map, across, down, kind]) => ({
    ...map,
    world: world(16384),
    view: { zoom: 128, center: half(16384) },
    polygon: ground(half(16384), 1, [
      (across * 16384) / map.width,
      (down * 16384) / 2 / map.height,
    ]),
    kind,
  }));
  for (const { input, shape } of await shapes(page, [...covering, ...edges])) {
    expect(shape.kind).toBe(input.kind);
    if (input.kind !== "cover") continue;
    const { width, height, stretch } = input;
    expect(shape.corners).toEqual([false, false, false, false]);
    expect(shape.arrow).toBeNull();
    expect(shape.polygon).toHaveLength(4);
    for (const point of shape.polygon) {
      expect(reach(point, width, height, stretch)).toBeLessThanOrEqual(1);
      expect(reach(point, width, height, stretch)).toBeGreaterThan(0.88);
    }
    const xs = shape.polygon.map(([x]) => x);
    const ys = shape.polygon.map(([, y]) => y);
    expect(Math.max(...xs) - Math.min(...xs)).toBeGreaterThan(width * 0.88);
    expect(Math.max(...ys) - Math.min(...ys)).toBeGreaterThan(height * 0.88);
  }
});

test("a camera outside the world is located where it is, or pointed at", async ({
  page,
}) => {
  const camera: Point = [-40, 1000];
  const [beside, beyond] = await shapes(page, [
    { ...square, ...whole(2000), polygon: ground(camera) },
    { ...native, ...whole(2000), polygon: ground(camera) },
  ]);
  if (!beside || !beyond) throw new Error("Shape missing");
  // The letterboxed square still shows this place; the native diamond does not.
  expect(beside.shape.kind).toBe("spot");
  expect(beside.shape.polygon).toEqual([]);
  expect(
    distance(middle(beside.shape.box), project(camera, beside.input)),
  ).toBeLessThan(0.71);
  expect(beyond.shape.kind).toBe("arrow");
});

test("a view off the visible minimap becomes an arrow at the border, pointing at it", async ({
  page,
}) => {
  const base = {
    world: [2000, 2000] as Point,
    view: { zoom: 6, center: [1050, 900] as Point },
  };
  const inputs = [native, square, overview].flatMap((map) =>
    [0.97, 1.03, 4].flatMap((far) =>
      Array.from({ length: 12 }, (_, index) => {
        const across = Math.cos((index * Math.PI) / 6);
        const down = Math.sin((index * Math.PI) / 6);
        const edge = map.stretch
          ? Math.abs(across) + Math.abs(down)
          : Math.max(Math.abs(across), Math.abs(down));
        const target: Point = [
          map.width * (0.5 + ((across / edge) * far) / 2),
          map.height * (0.5 + ((down / edge) * far) / 2),
        ];
        const camera = unproject(target, { ...base, ...map });
        return { ...base, ...map, polygon: ground(camera), target, far, index };
      }),
    ),
  );
  for (const { input, shape } of await shapes(page, inputs)) {
    const { target, far, index, width, height, stretch } = input;
    expect([far, index, shape.kind === "arrow"]).toEqual([far, index, far > 1]);
    if (far < 1) continue;
    const outline = shape.arrow ?? [];
    const tip = outline[0];
    if (!tip || outline.length < 3) throw new Error("Arrow outline missing");
    const centre: Point = [
      outline.reduce((sum, [x]) => sum + x, 0) / outline.length,
      outline.reduce((sum, [, y]) => sum + y, 0) / outline.length,
    ];
    const pointing =
      ((tip[0] - centre[0]) * (target[0] - centre[0]) +
        (tip[1] - centre[1]) * (target[1] - centre[1])) /
      (distance(tip, centre) * distance(target, centre));
    expect(pointing).toBeGreaterThan(0.99);
    for (const point of outline)
      expect(reach(point, width, height, stretch)).toBeLessThan(1);
    expect(reach(tip, width, height, stretch)).toBeGreaterThan(0.75);
    const size = Math.max(
      ...outline.flatMap((a) => outline.map((b) => distance(a, b))),
    );
    expect(size).toBeGreaterThan(10);
    expect(size).toBeLessThan(18);
  }
});
