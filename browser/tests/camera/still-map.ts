import { expect, type Page } from "@playwright/test";
import { PNG } from "pngjs";

/** What #minimap-marker paints now, in page CSS px; boxes leave strokes out. */
export type Drawn = {
  kind: string | null;
  pulse: string | null;
  /** Centre and size of everything painted. */
  x: number;
  y: number;
  width: number;
  height: number;
  /** Centre of the smallest painted shape: the exact place of a spot. */
  spot: [number, number];
  /** Screen width of each stroke across and down, sorted. */
  strokes: [number, number][];
  /** How many painted shapes have a fill, and how many are not closed. */
  filled: number;
  open: number;
};

/** `window.still`: the served game page with no WASM client to move it. */
export type Still = {
  /** The drawn map in page CSS px, letterboxing left out. */
  map: { left: number; top: number; width: number; height: number };
  /** The world point that many px from the centre of the whole minimap. */
  at(right: number, down: number): [number, number];
  present(x: number, y: number, zoom?: number, world?: number): void;
  actors(records: ArrayLike<number>): void;
  /** Actors on every half px of a rectangle centred there. */
  block(
    place: [number, number],
    size: [number, number],
    actor: { kind: number; owner: number; colour: number },
  ): number[];
  /** Multiplies the minimap zoom, as the wheel does. */
  wheel(factor: number): void;
  /** The world point a click on the map asked the camera to jump to. */
  jump(): [number, number];
  drawn(): Drawn;
  frames(count: number): Promise<void>;
};

/** Loads the production page, its minimap alone driven by the test. */
export async function stillMap(
  page: Page,
  frame: "native" | "square" | "overview",
) {
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.route("**/pkg/aoe_client.js", (route) =>
    route.fulfill({
      contentType: "text/javascript",
      body: "export default async () => {};",
    }),
  );
  await page.route("**/asset-pack/**", (route) =>
    route.fulfill({ status: 404, body: "" }),
  );
  await page.goto("/?play=1");
  await expect
    .poll(() => page.evaluate(() => Reflect.has(window, "aoeNavigation")))
    .toBe(true);
  await page.evaluate(async (frame) => {
    const element = (id: string) => {
      const node = document.getElementById(id);
      if (!node) throw new Error(`#${id} is missing`);
      return node;
    };
    const frames = async (count: number) => {
      for (let at = 0; at < count; at++)
        await new Promise((resolve) => requestAnimationFrame(resolve));
    };
    const navigation = Reflect.get(window, "aoeNavigation") as {
      present(state: Float64Array): void;
      frame(state: Float64Array): Float64Array;
      actors(records: Float64Array): void;
      fog(disabled: boolean, cells: number[]): void;
    };
    const svg = element("minimap-map");
    if (frame === "native") {
      // What panel-frame.js and ui-art.js set once the menu art has loaded.
      element("bottom-panel").dataset["frame"] = "native";
      for (const [id, width, height] of [
        ["commands", "276.5px", "189.5px"],
        ["minimap", "413.5px", "201.5px"],
      ] as const) {
        element(id).style.setProperty("--frame-width", width);
        element(id).style.setProperty("--frame-height", height);
      }
      svg.setAttribute("preserveAspectRatio", "none");
    }
    if (frame === "overview") element("overview-toggle").click();
    // White land, so ink and gaps are measurable pixels.
    navigation.fog(true, []);
    element("minimap-frame").firstElementChild?.setAttribute("fill", "#ffffff");
    await frames(3);

    const rect = svg.getBoundingClientRect();
    const side = Math.min(rect.width, rect.height);
    const width = frame === "native" ? rect.width : side;
    const height = frame === "native" ? rect.height : side;
    const map = {
      left: rect.left + (rect.width - width) / 2,
      top: rect.top + (rect.height - height) / 2,
      width,
      height,
    };
    const state = (x: number, y: number, zoom = 1, world = 2000) =>
      new Float64Array([
        ...[world, world, x, y, NaN, NaN],
        ...[1280, 720, zoom, 0, 1],
      ]);
    const at = (right: number, down: number): [number, number] => [
      2000 * (0.5 + right / width + down / height),
      2000 * (0.5 + down / height - right / width),
    ];
    const still: Still = {
      map,
      at,
      present(x, y, zoom, world) {
        navigation.present(state(x, y, zoom, world));
      },
      actors(records) {
        navigation.actors(Float64Array.from(records));
      },
      block([right, down], [wide, high], { kind, owner, colour }) {
        const records = [];
        for (let across = 0.25; across < wide; across += 0.5)
          for (let below = 0.25; below < high; below += 0.5)
            records.push(
              kind,
              ...at(right + across - wide / 2, down + below - high / 2),
              owner,
              colour,
            );
        return records;
      },
      wheel(factor) {
        svg.dispatchEvent(
          new WheelEvent("wheel", {
            deltaY: -Math.log(factor) / 0.0015,
            cancelable: true,
          }),
        );
      },
      jump() {
        const output = navigation.frame(state(1000, 1000));
        return [output[2] ?? NaN, output[3] ?? NaN];
      },
      drawn() {
        const root = element("minimap-marker");
        const parts = [root, ...root.querySelectorAll("*")].filter(
          (node): node is SVGGeometryElement =>
            node instanceof SVGGeometryElement &&
            !node.closest("defs,clipPath,mask,symbol") &&
            node.checkVisibility({
              checkOpacity: true,
              checkVisibilityCSS: true,
            }) &&
            node.getBBox().width + node.getBBox().height > 0,
        );
        const boxes = parts.map((node) => node.getBoundingClientRect());
        const smallest = boxes.reduce<DOMRect | null>(
          (least, box) =>
            least && least.width * least.height <= box.width * box.height
              ? least
              : box,
          null,
        );
        if (!smallest) throw new Error("#minimap-marker paints nothing");
        const left = Math.min(...boxes.map((box) => box.left));
        const right = Math.max(...boxes.map((box) => box.right));
        const top = Math.min(...boxes.map((box) => box.top));
        const bottom = Math.max(...boxes.map((box) => box.bottom));
        const strokes = parts.flatMap((node): [number, number][] => {
          const style = getComputedStyle(node);
          const line = parseFloat(style.strokeWidth);
          const matrix = node.getScreenCTM();
          if (style.stroke === "none" || !(line > 0) || !matrix) return [];
          const free = style.vectorEffect === "non-scaling-stroke";
          return [
            [
              free ? line : line * Math.hypot(matrix.a, matrix.b),
              free ? line : line * Math.hypot(matrix.c, matrix.d),
            ].map((value) => Number(value.toFixed(2))) as [number, number],
          ];
        });
        const filled = parts.filter((node) => {
          const { fill, fillOpacity } = getComputedStyle(node);
          return fill !== "none" && parseFloat(fillOpacity) > 0;
        });
        const open = parts.filter((node) =>
          (node.getAttribute("d") ?? "Mz")
            .split(/(?=M)/)
            .some((outline) => !/z\s*$/i.test(outline)),
        );
        return {
          filled: filled.length,
          open: open.length,
          kind: root.getAttribute("data-kind"),
          pulse: root.getAttribute("data-pulse"),
          x: (left + right) / 2,
          y: (top + bottom) / 2,
          width: right - left,
          height: bottom - top,
          spot: [
            smallest.left + smallest.width / 2,
            smallest.top + smallest.height / 2,
          ],
          strokes: strokes.sort((a, b) => a[0] - b[0] || a[1] - b[1]),
        };
      },
      frames,
    };
    Reflect.set(window, "still", still);
  }, frame);
}

/** The px of the page that the marker changes inside the rectangle of the map,
 * and beyond it by more than half of its widest stroke. */
export async function spilled(page: Page) {
  // The buttons beside the map are painted a little differently while something
  // under them is animated; they are not the marker.
  await page.addStyleTag({
    content:
      "#map-navigation,#map-navigation button,#minimap-zoom,#minimap-title,#minimap-close{visibility:hidden!important}",
  });
  const shot = async (visibility: "visible" | "hidden") => {
    const map = await page.evaluate((visibility) => {
      const root = document.getElementById("minimap-marker");
      if (!root) throw new Error("#minimap-marker is missing");
      root.style.visibility = visibility;
      return (Reflect.get(window, "still") as Still).map;
    }, visibility);
    // A layer that was just scaled is drawn again, sharper, some frames later.
    let bytes = await page.screenshot({ scale: "css" });
    for (let tries = 0, calm = 0; calm < 2; tries++) {
      if (tries > 40) throw new Error("the page never stands still");
      await page.waitForTimeout(100);
      const next = await page.screenshot({ scale: "css" });
      calm = next.equals(bytes) ? calm + 1 : 0;
      bytes = next;
    }
    return { map, image: PNG.sync.read(bytes) };
  };
  const bare = await shot("hidden");
  const { map, image } = await shot("visible");
  const seen = { inside: 0, outside: 0 };
  for (let at = 0; at < image.data.length; at += 4) {
    const before = bare.image.data.subarray(at, at + 3);
    // Text elsewhere on the page moves by one level when a layer comes or goes.
    const same = before.every(
      (value, channel) =>
        Math.abs(value - (image.data[at + channel] ?? 0)) <= 2,
    );
    if (same) continue;
    const x = (at / 4) % image.width;
    const y = Math.floor(at / 4 / image.width);
    const beyond =
      x < map.left - 5 ||
      y < map.top - 5 ||
      x > map.left + map.width + 4 ||
      y > map.top + map.height + 4;
    seen[beyond ? "outside" : "inside"]++;
  }
  return seen;
}

/** The colour of the page at a CSS px, from one screenshot. */
export async function pixels(page: Page) {
  const image = PNG.sync.read(await page.screenshot({ scale: "css" }));
  return (x: number, y: number) => {
    const at = (Math.floor(y) * image.width + Math.floor(x)) * 4;
    return {
      red: image.data[at] ?? 0,
      green: image.data[at + 1] ?? 0,
      blue: image.data[at + 2] ?? 0,
    };
  };
}
