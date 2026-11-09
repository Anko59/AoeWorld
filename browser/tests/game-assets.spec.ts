import { expect, type Page, type Route } from "@playwright/test";
import { gameAssets } from "./game-assets.js";
import { test } from "../server.js";

// Unit-level checks of the synthetic asset pack: no browser and no server.
type Arguments = Parameters<typeof gameAssets>;
type Fulfilled = NonNullable<Parameters<Route["fulfill"]>[0]>;
type Handler = (route: Route) => Promise<void>;
type Fetch = (file: string) => Promise<Fulfilled>;
type Frame = { source: string };

const pages = ["fixture-color.png", "fixture-shadow.png", "fixture-mask.png"];

/**
 * One synthetic `gameAssets` call on a page that only records its routes. The
 * result asks that call's own `asset-pack` handler for a file.
 */
async function install(
  additionalSprites?: Arguments[2],
  omitSprites?: Arguments[3],
): Promise<Fetch> {
  const handlers = new Map<string, Handler>();
  const page = {
    route: async (pattern: string, handler: Handler) => {
      handlers.set(pattern, handler);
    },
  } as unknown as Page;
  await gameAssets(page, true, additionalSprites, omitSprites);
  return async (file) => {
    const url = `http://fixture.test/asset-pack/${file}`;
    const served: Fulfilled[] = [];
    await handlers.get("**/asset-pack/*")?.({
      request: () => ({ url: () => url }),
      fulfill: async (response: Fulfilled) => {
        served.push(response);
      },
    } as unknown as Route);
    const [response] = served;
    if (!response || served.length !== 1)
      throw new Error(`${file}: served ${served.length} responses`);
    return response;
  };
}

async function png(fetch: Fetch, file: string): Promise<Buffer> {
  const { body } = await fetch(file);
  if (!Buffer.isBuffer(body)) throw new Error(`${file}: not served as bytes`);
  return body;
}

async function frames(fetch: Fetch): Promise<Frame[]> {
  const manifest: { frames: Frame[] } = (await fetch("manifest.json")).json;
  return manifest.frames;
}

const sprite = (id: number) => (frame: Frame) =>
  frame.source.endsWith(`:${id}`);

test("synthetic atlas pages are encoded once per worker, not once per call", async () => {
  const calls = [
    await install(),
    await install(),
    await install([[2223, 3]], [435]),
  ];
  for (const file of pages) {
    const bodies = await Promise.all(calls.map((fetch) => png(fetch, file)));
    // A Set keeps distinct objects, so its size is the number of encoded
    // buffers behind these three calls: one shared page, not one per call.
    expect(new Set(bodies).size, file).toBe(1);
  }
});

test("sprite arguments change the manifest and never a page's bytes", async () => {
  const plain = await install();
  const changed = await install([[2223, 3]], [435]);
  for (const file of pages) {
    const bytes = await png(changed, file);
    expect(bytes.equals(await png(plain, file)), file).toBe(true);
  }
  const before = await frames(plain);
  const after = await frames(changed);
  expect(before.filter(sprite(2223))).toHaveLength(0);
  expect(after.filter(sprite(2223))).toHaveLength(3);
  expect(before.filter(sprite(435))).toHaveLength(4);
  expect(after.filter(sprite(435))).toHaveLength(0);
  // Every other frame is still there, unchanged and in the same order.
  expect(after.filter((frame) => !sprite(2223)(frame))).toEqual(
    before.filter((frame) => !sprite(435)(frame)),
  );
});
