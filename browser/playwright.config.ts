import { defineConfig, devices } from "@playwright/test";
import { serverUrls } from "./server.js";

// Keep preset viewports and browser identity, with the Linux browser's native UA.
const nativeDevice = (name: "Desktop Chrome" | "Desktop Firefox") => {
  const device = { ...devices[name] };
  Reflect.deleteProperty(device, "userAgent");
  return device;
};

// Projects differ only in how the browser draws the canvas. `webgpu` runs every
// spec. `browser-defaults` and `canvas` run only the renderer-dependent specs
// (renderer selection, fallbacks, pixel reads, per-project branches), so the
// DOM, UI, request and flow specs are not repeated. A new spec is agnostic
// unless it is added to this list; if unsure, add it. Matches are anchored to
// whole file names, never substrings (`play.spec.ts` alone must not match
// every `*.play.spec.ts`).
const rendererDependent = [
  /[\\/]village[\\/]vassal[\\/]vassal\.play\.spec\.ts$/,
  /[\\/]village[\\/]castle[\\/]castle\.play\.spec\.ts$/,
  /[\\/]play\.spec\.ts$/,
  /(?:^|[\\/.])shared-surfaces\.spec\.ts$/,
  /[\\/]source-creator\.spec\.ts$/,
  /[\\/]camera[\\/]hooks\.play\.spec\.ts$/,
  /[\\/]memory[\\/]exploration\.spec\.ts$/,
  /[\\/]rendering[\\/][^\\/]+\.play\.spec\.ts$/,
  /[\\/]landscape[\\/]forest\.play\.spec\.ts$/,
  /[\\/]village[\\/]feudal\.play\.spec\.ts$/,
];

export default defineConfig({
  testDir: "./tests",
  outputDir: "../.cache/playwright-results",
  timeout: 30_000,
  expect: { timeout: 10_000 },
  retries: 0,
  // One worker per game server: the harness starts AOE_E2E_WORKERS servers and
  // lists them in AOE_BASE_URLS (see server.ts), so no two workers share a
  // controller lease or the server-wide map and unit state (#388).
  workers: serverUrls().length,
  reporter: [["list"]],
  use: {
    ...nativeDevice("Desktop Chrome"),
    baseURL: serverUrls()[0],
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  projects: [
    {
      name: "firefox-webgpu",
      testMatch: /[\\/]rendering[\\/]recovery[\\/]firefox\.play\.spec\.ts$/,
      use: {
        ...nativeDevice("Desktop Firefox"),
        launchOptions: {
          firefoxUserPrefs: {
            "dom.webgpu.enabled": true,
            "webgl.force-enabled": true,
          },
        },
      },
    },
    {
      name: "firefox-webgl2",
      testMatch:
        /[\\/]rendering[\\/](?:startup|placement)\.play\.spec\.ts$|[\\/]village[\\/](?:(?:input|formations|vassal)[\\/][^\\/]+|production[\\/](?:rally|reseed|auto-scout)|castle[\\/]castle)\.play\.spec\.ts$|[\\/]proofs[\\/]ui-panel\.spec\.ts$/,
      use: {
        ...nativeDevice("Desktop Firefox"),
        launchOptions: { firefoxUserPrefs: { "webgl.force-enabled": true } },
      },
    },
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
        /(?:^|[\\/.])shared-surfaces\.spec\.ts$|[\\/]village[\\/](?:feudal|vassal[\\/]vassal|castle[\\/]castle)\.play\.spec\.ts$|[\\/]rendering[\\/](?:bigview-interaction|bigview-population)\.play\.spec\.ts$/,
      use: { launchOptions: { args: [] } },
    },
    {
      name: "browser-defaults",
      testMatch: rendererDependent,
      use: { launchOptions: { args: [] } },
    },
  ],
});
