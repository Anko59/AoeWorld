import { expect, test, type Page } from "@playwright/test";
import {
  buildGeneratedGameAssets,
  gameAssets,
  generatedGameAssets,
} from "../game-assets.js";

test("worker fixture cache preserves every byte and per-page pack selection", async () => {
  const original = buildGeneratedGameAssets();
  const cached = generatedGameAssets();
  expect(generatedGameAssets()).toBe(cached);
  expect(cached.manifest).toEqual(original.manifest);
  for (const name of ["colored", "shadows", "empty"] as const) {
    expect(
      cached[name].equals(original[name]),
      `${name} PNG bytes`,
    ).toBeTruthy();
  }

  type Handler = Parameters<Page["route"]>[1];
  const fake = (localPack: boolean) => {
    const probes: string[] = [];
    const routes: Array<{ pattern: string; handler: Handler }> = [];
    const page = {
      request: {
        get: async (url: string) => {
          probes.push(url);
          return { ok: () => localPack };
        },
      },
      route: async (pattern: string, handler: Handler) => {
        routes.push({ pattern, handler });
      },
    } as unknown as Page;
    return { page, probes, routes };
  };
  const local = fake(true);
  expect(await gameAssets(local.page)).toBe("local AoE II pack");
  expect(await gameAssets(local.page)).toBe("local AoE II pack");
  expect(local.probes).toEqual([
    "/asset-pack/manifest.json",
    "/asset-pack/manifest.json",
  ]);
  expect(local.routes).toHaveLength(0);

  for (const forced of [false, true]) {
    const first = fake(false);
    const second = fake(false);
    for (const entry of [first, second]) {
      expect(await gameAssets(entry.page, forced)).toBe(
        "generated CI fixtures",
      );
      expect(entry.probes).toEqual(forced ? [] : ["/asset-pack/manifest.json"]);
      expect(entry.routes).toHaveLength(1);
      const registration = entry.routes[0];
      if (!registration) throw new Error("Generated route absent");
      expect(registration.pattern).toBe("**/asset-pack/*");
      for (const [file, expected] of [
        ["manifest.json", { json: original.manifest }],
        [
          "fixture-color.png",
          { contentType: "image/png", body: original.colored },
        ],
        [
          "fixture-shadow.png",
          { contentType: "image/png", body: original.shadows },
        ],
        [
          "fixture-mask.png",
          { contentType: "image/png", body: original.empty },
        ],
      ] as const) {
        let response: unknown;
        const route = {
          request: () => ({ url: () => `http://localhost/asset-pack/${file}` }),
          fulfill: async (options: unknown) => {
            response = options;
          },
        } as Parameters<Handler>[0];
        await registration.handler(route, route.request());
        expect(response).toEqual(expected);
      }
    }
  }
});
