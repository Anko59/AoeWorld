import { defineConfig, devices } from "@playwright/test";
import { serverUrls } from "./server.js";

// Projects differ only in how the browser draws the canvas. `webgpu` runs every
// spec. `browser-defaults` and `canvas` run only the renderer-dependent specs
// (renderer selection, fallbacks, pixel reads, per-project branches), so the
// DOM, UI, request and flow specs are not repeated. A new spec is agnostic
// unless it is added to this list; if unsure, add it. Matches are anchored to
// whole file names, never substrings (`play.spec.ts` alone must not match
// every `*.play.spec.ts`).
const rendererDependent = [
  /[\\/]play\.spec\.ts$/,
  /(?:^|[\\/.])shared-surfaces\.spec\.ts$/,
  /[\\/]source-creator\.spec\.ts$/,
  /[\\/]camera[\\/](?:hooks|navigation)\.play\.spec\.ts$/,
  /[\\/](?:rendering|assets)[\\/][^\\/]+\.play\.spec\.ts$/,
  /[\\/]landscape[\\/]forest\.play\.spec\.ts$/,
  /[\\/]village[\\/]selection\.play\.spec\.ts$/,
];

export default defineConfig({
  testDir: "./tests",
  timeout: 30_000,
  expect: { timeout: 10_000 },
  retries: 0,
  // One worker per game server: the harness starts AOE_E2E_WORKERS servers and
  // lists them in AOE_BASE_URLS (see server.ts), so no two workers share a
  // controller lease or the server-wide map and unit state (#388).
  workers: serverUrls().length,
  reporter: [["list"]],
  use: {
    ...devices["Desktop Chrome"],
    baseURL: serverUrls()[0],
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  projects: [
    {
      name: "webgpu",
      use: {
        launchOptions: {
          args: [
            "--use-angle=swiftshader",
            "--enable-unsafe-swiftshader",
            "--enable-unsafe-webgpu",
            "--ignore-gpu-blocklist",
            "--enable-gpu",
            "--enable-features=Vulkan",
            "--use-vulkan=swiftshader",
            "--use-webgpu-adapter=swiftshader",
            "--disable-vulkan-surface",
          ],
        },
      },
    },
    {
      name: "canvas",
      testMatch:
        /(?:^|[\\/.])shared-surfaces\.spec\.ts$|[\\/]village[\\/]selection\.play\.spec\.ts$/,
      use: { launchOptions: { args: [] } },
    },
    {
      name: "browser-defaults",
      testMatch: rendererDependent,
      use: { launchOptions: { args: [] } },
    },
  ],
});
