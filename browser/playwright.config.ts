import { defineConfig, devices } from "@playwright/test";

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
      testMatch: /shared-surfaces\.spec\.ts/,
      use: { launchOptions: { args: [] } },
    },
    {
      name: "browser-defaults",
      testMatch: /play\.spec\.ts|shared-surfaces\.spec\.ts/,
      use: { launchOptions: { args: [] } },
    },
  ],
});
