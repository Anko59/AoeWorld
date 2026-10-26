import { test as base } from "@playwright/test";

// One disposable game server per Playwright worker (#388). The harness starts
// them on distinct ports and lists the URLs in AOE_BASE_URLS; a worker uses the
// server at its `parallelIndex`, which stays below the worker count even when a
// failed test restarts the worker (its `workerIndex` keeps growing). Without the
// variable the run uses one server at AOE_BASE_URL.
export const serverUrls = (): string[] => {
  const list = process.env["AOE_BASE_URLS"];
  const urls = list
    ? list.split(",").filter((url) => url !== "")
    : [process.env["AOE_BASE_URL"] ?? "http://127.0.0.1:8080"];
  if (urls.length === 0) throw new Error("AOE_BASE_URLS lists no server");
  return urls;
};

export const test = base.extend({
  // Playwright reads the fixture dependencies from this destructuring pattern.
  // eslint-disable-next-line no-empty-pattern
  baseURL: async ({}, use, testInfo) => {
    const urls = serverUrls();
    const url = urls[testInfo.parallelIndex];
    if (url === undefined) {
      throw new Error(
        `worker ${testInfo.parallelIndex} has no server among ${urls.length}`,
      );
    }
    await use(url);
  },
});

// A long-lived SwiftShader worker can stall Browser.setDownloadBehavior when
// the next context is created after a rendering-heavy spec. Give the affected
// village flow its own browser lifetime, preserving renderer flags and traces.
export const isolatedTest = test.extend({
  browser: [
    async (
      { playwright, browserName, launchOptions, headless, channel },
      use,
    ) => {
      const browser = await playwright[browserName].launch({
        ...launchOptions,
        headless,
        channel,
      });
      try {
        await use(browser);
      } finally {
        await browser.close();
      }
    },
    { scope: "worker" },
  ],
});
