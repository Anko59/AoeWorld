import { expect, type Page } from "@playwright/test";
import { PNG } from "pngjs";

export type Layout = "native" | "square" | "overview";
export const layouts = ["native", "square", "overview"] as const;
/** The chart's flat colours. */
export const chart = { centre: [42, 33, 22], edge: [26, 20, 12] };
/** The body of a raster response, or the HTTP status that refuses it. */
export type Reply = Uint8Array<ArrayBuffer> | number;
type Context = CanvasRenderingContext2D;

type Navigation = {
  fog: {
    (disabled: boolean, pixels: ArrayLike<number>): void;
    /** The 256-cell mask that the strategic plane draws. */
    canvas: HTMLCanvasElement;
    revision: number;
    disabled: boolean;
    explored(x: number, y: number): boolean;
    /** The decoded raster of the land; null empties the composed canvas. */
    land(raster: object | null): void;
  };
  map(hash: Uint8Array): Promise<void>;
  present(state: Float64Array): void;
  frame(state: Float64Array): Float64Array;
  actors(records: Float64Array): void;
};

/** `window.land`: the served game page with no WASM client to drive it. */
export type Land = {
  navigation: Navigation;
  /** The drawn map in page CSS px, letterboxing left out. */
  map: { left: number; top: number; width: number; height: number };
  /** An AWMM body; each sample's byte is class | shade << 4. */
  raster(
    samples: number,
    cell: (x: number, y: number) => number,
  ): Uint8Array<ArrayBuffer>;
  /** Opens a new map; a raster request without a reply gets a 404. */
  open(replies: {
    coarse?: Reply | Promise<Reply>;
    fine?: Reply | Promise<Reply>;
  }): Promise<void>;
  /** A reply the test gives later. */
  hold(): { reply: Promise<Reply>; release(reply: Reply): void };
  /** Waits for #minimap's data-raster. */
  until(raster: string): Promise<void>;
  /** Fog of every cell: 0 unexplored, 1 remembered, 2 visible. */
  fog(state: (x: number, y: number) => number): void;
  /** What the minimap may show of the land: 512x512, two pixels a cell. */
  composed(): ImageData;
  /** Draw calls into the composed canvas since the page opened. */
  paints(): number;
  /** Puts the camera on a tile of the 256x256 world. */
  present(x: number, y: number): void;
  /** Where the last click asked the camera to jump to. */
  jump(): [number, number];
  /** Multiplies the minimap zoom, as the wheel does. */
  wheel(factor: number): void;
  frames(count: number): Promise<void>;
};

/** Loads the production page, its minimap alone driven by the test. */
export async function landPage(
  page: Page,
  layout: Layout = "square",
): Promise<Land["map"]> {
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
  await page.addInitScript(() => {
    let paints = 0;
    const drawing = CanvasRenderingContext2D.prototype;
    for (const name of ["putImageData", "drawImage"] as const) {
      const draw = drawing[name];
      const counted = function (this: Context, ...args: unknown[]): unknown {
        if (this.canvas.closest("#minimap-known")) paints++;
        return Reflect.apply(draw, this, args);
      };
      Reflect.set(drawing, name, counted);
    }
    Reflect.set(window, "landPaints", () => paints);
  });
  await page.goto("/?play=1");
  await expect
    .poll(() => page.evaluate(() => Reflect.has(window, "aoeNavigation")))
    .toBe(true);
  // HTML buttons and text above the map are not what these tests look at.
  await page.addStyleTag({
    content:
      "#map-navigation,#map-navigation button,#minimap-zoom,#minimap-title,#minimap-close{visibility:hidden!important}",
  });
  return page.evaluate(async (layout) => {
    const element = (id: string) => {
      const node = document.getElementById(id);
      if (!node) throw new Error(`#${id} is missing`);
      return node;
    };
    const frames = async (count: number) => {
      for (let at = 0; at < count; at++)
        await new Promise((resolve) => requestAnimationFrame(resolve));
    };
    const navigation = Reflect.get(window, "aoeNavigation") as Navigation;
    const panel = element("minimap");
    const svg = element("minimap-map");
    if (layout === "native") {
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
    } else if (layout === "overview") element("overview-toggle").click();
    await frames(3);

    const replies = new Map<string, Parameters<Land["open"]>[0]>();
    const fetched = window.fetch.bind(window);
    window.fetch = async (input, init) => {
      const url = input instanceof Request ? input.url : String(input);
      const [, hash = "", coarse] =
        /\/maps\/([0-9a-f]{64})\/minimap(\/coarse)?$/.exec(url) ?? [];
      const stages = replies.get(hash);
      if (!stages) return fetched(input, init);
      const reply = (await (coarse ? stages.coarse : stages.fine)) ?? 404;
      return typeof reply === "number"
        ? new Response(null, { status: reply })
        : new Response(reply);
    };

    const rect = svg.getBoundingClientRect();
    const side = Math.min(rect.width, rect.height);
    const width = layout === "native" ? rect.width : side;
    const height = layout === "native" ? rect.height : side;
    const state = (x: number, y: number) =>
      Float64Array.of(256, 256, x, y, NaN, NaN, 1280, 720, 1, 0, 1);
    let opened = 0;
    const land: Land = {
      navigation,
      map: {
        left: rect.left + (rect.width - width) / 2,
        top: rect.top + (rect.height - height) / 2,
        width,
        height,
      },
      raster(samples, cell) {
        const bytes = new Uint8Array(8 + samples * samples);
        bytes.set([65, 87, 77, 77, samples & 255, samples >> 8]);
        for (let at = 0; at < samples * samples; at++)
          bytes[8 + at] = cell(at % samples, Math.floor(at / samples));
        return bytes;
      },
      async open(stages) {
        const bytes = new Uint8Array(32).fill(0xab);
        opened++;
        bytes[31] = opened & 255;
        const hash = Array.from(bytes, (byte) =>
          byte.toString(16).padStart(2, "0"),
        ).join("");
        replies.set(hash, stages);
        await navigation.map(bytes);
      },
      hold() {
        let resolve: (reply: Reply) => void = () => undefined;
        const reply = new Promise<Reply>((settle) => {
          resolve = settle;
        });
        return { reply, release: (value) => resolve(value) };
      },
      async until(raster) {
        for (let at = 0; panel.dataset["raster"] !== raster; at++) {
          if (at > 300)
            throw new Error(`data-raster stays ${panel.dataset["raster"]}`);
          await frames(1);
        }
      },
      fog(cell) {
        const pixels: number[] = [];
        for (let at = 0; at < 65536; at++) {
          const value = cell(at & 255, at >> 8);
          if (value) pixels.push(at | (value << 16));
        }
        navigation.fog(false, Uint32Array.from(pixels));
      },
      composed() {
        const canvas = document.querySelector<HTMLCanvasElement>(
          "#minimap-known canvas",
        );
        const pixels = canvas
          ?.getContext("2d")
          ?.getImageData(0, 0, canvas.width, canvas.height);
        if (!pixels) throw new Error("#minimap-known holds no composed land");
        return pixels;
      },
      paints: () => (Reflect.get(window, "landPaints") as () => number)(),
      present(x, y) {
        navigation.present(state(x, y));
      },
      jump() {
        const output = navigation.frame(state(128, 128));
        return [output[2] ?? NaN, output[3] ?? NaN];
      },
      wheel(factor) {
        svg.dispatchEvent(
          new WheelEvent("wheel", {
            deltaY: -Math.log(factor) / 0.0015,
            cancelable: true,
          }),
        );
      },
      frames,
    };
    land.present(128, 128);
    Reflect.set(window, "land", land);
    await frames(2);
    return land.map;
  }, layout);
}

/** Page CSS px of the centre of a fog cell, on the whole minimap. */
export function place(
  map: Land["map"],
  [x, y]: readonly [number, number],
): [number, number] {
  return [
    map.left + map.width * (0.5 + (x - y) / 512),
    map.top + map.height * ((x + y + 1) / 512),
  ];
}

/** The minimap as the screen shows it; colours by page CSS px. */
export async function shot(page: Page) {
  const clip = await page.evaluate(() => {
    const { map } = Reflect.get(window, "land") as Land;
    const x = Math.floor(map.left);
    const y = Math.floor(map.top);
    return {
      x,
      y,
      width: Math.ceil(map.left + map.width) - x,
      height: Math.ceil(map.top + map.height) - y,
    };
  });
  // A layer that was just scaled is drawn again, sharper, some frames later, and
  // moves by a level or two: only a picture that stands still is compared.
  let bytes = await page.screenshot({ clip, scale: "css" });
  for (let tries = 0, calm = 0; calm < 2; tries++) {
    if (tries > 40) throw new Error("the minimap never stands still");
    await page.waitForTimeout(100);
    const next = await page.screenshot({ clip, scale: "css" });
    calm = next.equals(bytes) ? calm + 1 : 0;
    bytes = next;
  }
  const image = PNG.sync.read(bytes);
  return {
    image,
    colour([x, y]: readonly [number, number]): [number, number, number] {
      const at =
        (Math.floor(y - clip.y) * image.width + Math.floor(x - clip.x)) * 4;
      return [
        image.data[at] ?? 0,
        image.data[at + 1] ?? 0,
        image.data[at + 2] ?? 0,
      ];
    },
  };
}

/** Pixels of two screenshots that differ in any channel. */
export function differing(a: PNG, b: PNG) {
  if (a.width !== b.width || a.height !== b.height) return Infinity;
  let count = 0;
  for (let at = 0; at < a.data.length; at += 4)
    if (a.data.compare(b.data, at, at + 4, at, at + 4) !== 0) count++;
  return count;
}

/** Pixels on which no capture of one scene equals any capture of another. A
 * pixel that a renderer draws differently by chance in one capture still equals
 * the other scene in the second one; a difference that comes from the scene
 * stays, even where the captures of one scene disagree. */
export function contradicted(a: [PNG, PNG], b: [PNG, PNG]) {
  const [first] = a;
  for (const { width, height } of [a[1], ...b])
    if (width !== first.width || height !== first.height) return Infinity;
  const same = (x: PNG, y: PNG, at: number) =>
    x.data.compare(y.data, at, at + 4, at, at + 4) === 0;
  let count = 0;
  for (let at = 0; at < first.data.length; at += 4)
    if (!a.some((one) => b.some((other) => same(one, other, at)))) count++;
  return count;
}

/** The largest channel difference from the colour wanted. */
export function apart(colour: readonly number[], want: readonly number[]) {
  return Math.max(
    ...want.map((value, at) => Math.abs(value - (colour[at] ?? NaN))),
  );
}

/** WCAG relative luminance. */
export function luminance(colour: readonly number[]) {
  const [red = 0, green = 0, blue = 0] = colour.map((value) => {
    const part = value / 255;
    return part <= 0.04045 ? part / 12.92 : ((part + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
}

/** WCAG contrast of two colours, 1 or more. */
export function contrast(a: readonly number[], b: readonly number[]) {
  const [one, two] = [luminance(a) + 0.05, luminance(b) + 0.05] as const;
  return Math.max(one, two) / Math.min(one, two);
}
