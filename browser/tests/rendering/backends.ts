import type { Page } from "@playwright/test";

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
