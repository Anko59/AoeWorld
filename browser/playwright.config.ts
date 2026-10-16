import { defineConfig, devices } from "@playwright/test";

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
];

export default defineConfig({
  testDir: "./tests",
  timeout: 30_000,
  expect: { timeout: 10_000 },
  retries: 0,
  workers: 1,
  reporter: [["list"]],
  use: {
    ...devices["Desktop Chrome"],
    baseURL: process.env["AOE_BASE_URL"] ?? "http://127.0.0.1:8080",
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
      testMatch: /(?:^|[\\/.])shared-surfaces\.spec\.ts$/,
      use: { launchOptions: { args: [] } },
    },
    {
      name: "browser-defaults",
      testMatch: rendererDependent,
      use: { launchOptions: { args: [] } },
    },
  ],
});
