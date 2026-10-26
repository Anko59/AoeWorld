import type { Page } from "@playwright/test";

export async function profileGpu(page: Page) {
  await page.addInitScript(() => {
    const counts: Record<string, number> = {};
    const add = (key: string, n = 1) => {
      counts[key] = (counts[key] ?? 0) + n;
    };
    Reflect.set(window, "spriteProfile", counts);
    Reflect.set(window, "spriteIntervals", []);
    for (const [type, methods] of [
      [
        "GPUDevice",
        [
          "createBuffer",
          "createBindGroup",
          "createRenderPipeline",
          "createTexture",
          "createQuerySet",
        ],
      ],
      ["GPUQueue", ["writeBuffer", "writeTexture", "submit"]],
      ["GPUBuffer", ["mapAsync"]],
      ["GPUTexture", ["createView"]],
      [
        "GPURenderPassEncoder",
        ["setPipeline", "setBindGroup", "draw", "drawIndexed"],
      ],
      [
        "WebGL2RenderingContext",
        [
          "bufferData",
          "bufferSubData",
          "texSubImage3D",
          "useProgram",
          "drawArraysInstanced",
          "finish",
          "readPixels",
          "fenceSync",
        ],
      ],
    ] as const) {
      const ctor = Reflect.get(window, type) as
        { prototype: object } | undefined;
      if (!ctor) continue;
      for (const name of methods) {
        const original = Reflect.get(ctor.prototype, name) as (
          ...args: unknown[]
        ) => unknown;
        Reflect.set(
          ctor.prototype,
          name,
          function (this: unknown, ...args: unknown[]) {
            add(`${type}.${name}`);
            if (name === "writeBuffer")
              add(
                "uploadBytes",
                Number(args[4] ?? (args[2] as ArrayBufferView).byteLength),
              );
            if (name === "draw") {
              const contour =
                Reflect.get(this as object, "label") ===
                "hidden units and resource hover contours";
              add(
                contour ? "contourInstances" : "worldInstances",
                Number(args[1] ?? 1),
              );
              add("instances", Number(args[1] ?? 1));
            }
            if (name === "drawArraysInstanced")
              add("instances", Number(args[3]));
            const start = performance.now();
            if (name === "submit" || name === "fenceSync") {
              const last = Reflect.get(window, "spriteLastSubmission") as
                number | undefined;
              if (last !== undefined)
                (Reflect.get(window, "spriteIntervals") as number[]).push(
                  start - last,
                );
              Reflect.set(window, "spriteLastSubmission", start);
            }
            const result = Reflect.apply(original, this, args);
            add(`${type}.${name}Ms`, performance.now() - start);
            return result;
          },
        );
      }
    }
  });
}

export async function readProfile(page: Page, reset = false) {
  return page.evaluate((clear) => {
    const counts = Reflect.get(window, "spriteProfile") as Record<
      string,
      number
    >;
    const intervals = [
      ...(Reflect.get(window, "spriteIntervals") as number[]),
    ].sort((a, b) => a - b);
    const result: Record<string, number | null> = {
      ...counts,
      submittedSamples: intervals.length,
      submittedFrameMs: intervals[Math.floor(intervals.length / 2)] ?? null,
      submittedFrameP95Ms:
        intervals[Math.floor(intervals.length * 0.95)] ?? null,
    };
    if (clear) {
      for (const key of Object.keys(counts))
        Reflect.deleteProperty(counts, key);
      Reflect.set(window, "spriteIntervals", []);
      Reflect.deleteProperty(window, "spriteLastSubmission");
    }
    return result;
  }, reset);
}

// Profile an actual submitted frame even when the unchanged scene is cached.
// The existing restoration event clears only the last-frame cache; it does
// not recreate the device, atlas or prepared scene. Primary timings stay idle.
export async function sampleOneSubmission(page: Page) {
  await readProfile(page, true);
  await page.evaluate(() => {
    document
      .getElementById("scene")
      ?.dispatchEvent(new Event("aoe-renderer-restored"));
  });
  await page.waitForFunction(() => {
    const counts = Reflect.get(window, "spriteProfile") as Record<
      string,
      number
    >;
    return (
      (counts["GPUQueue.submit"] ??
        counts["WebGL2RenderingContext.fenceSync"] ??
        0) > 0
    );
  });
  return readProfile(page);
}
