import type { Page } from "@playwright/test";
import { test } from "../../server.js";

// Removing navigator.gpu does not undo Vulkan/WebGPU process flags. Keep the
// normal context/trace fixtures, but launch forced fallback cases in a worker
// with the normal compositor; CI captures otherwise stalled for 15 seconds.
export const fallbackTest = test.extend({
  browser: [
    async (
      { playwright, browserName, launchOptions, headless, channel },
      use,
    ) => {
      const browser = await playwright[browserName].launch({
        ...launchOptions,
        args: [],
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

export async function denyWebGl(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const original = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = function (
      this: HTMLCanvasElement,
      type: string,
      ...args: unknown[]
    ) {
      if (type === "webgl2") return null;
      return Reflect.apply(original, this, [type, ...args]);
    } as typeof original;
  });
}

export async function forceWebGl(page: Page): Promise<void> {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "gpu", { value: undefined });
  });
}

// Missing WebGPU alone now exercises WebGL2, not the software Canvas tier.
export async function forceCanvas(page: Page): Promise<void> {
  await forceWebGl(page);
  await denyWebGl(page);
}
