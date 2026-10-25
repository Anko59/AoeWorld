import type { Page } from "@playwright/test";

/** Count graphics commands without querying or synchronizing the GPU. */
export async function profileCoverage(page: Page) {
  await page.addInitScript(() => {
    const profile = {
      enabled: false,
      allocations: {} as Record<string, number>,
      releases: 0,
      draws: 0,
      instances: 0,
      synchronous: 0,
    };
    Reflect.set(window, "coverageProfile", profile);
    const prototype = WebGL2RenderingContext.prototype;
    for (const name of [
      "texImage2D",
      "deleteTexture",
      "drawArraysInstanced",
      "finish",
      "readPixels",
      "getBufferSubData",
    ]) {
      const original = Reflect.get(prototype, name) as (
        ...args: unknown[]
      ) => unknown;
      Reflect.set(
        prototype,
        name,
        function (this: WebGL2RenderingContext, ...args: unknown[]) {
          if (
            profile.enabled &&
            this.canvas instanceof HTMLCanvasElement &&
            this.canvas.id === "scene"
          ) {
            if (name === "texImage2D") {
              const size = `${String(args[3])}x${String(args[4])}`;
              profile.allocations[size] = (profile.allocations[size] ?? 0) + 1;
            } else if (name === "deleteTexture") profile.releases++;
            else if (name === "drawArraysInstanced") {
              profile.draws++;
              profile.instances += Number(args[3]);
            } else profile.synchronous++;
          }
          return original.apply(this, args);
        },
      );
    }
  });
}

export async function coverageProfile(page: Page, enable = false) {
  return page.evaluate((enabled) => {
    const profile = Reflect.get(window, "coverageProfile") as {
      enabled: boolean;
      allocations: Record<string, number>;
      releases: number;
      draws: number;
      instances: number;
      synchronous: number;
    };
    if (enabled) profile.enabled = true;
    return profile;
  }, enable);
}
