import { expect } from "@playwright/test";
import { test } from "../../server.js";

test("actor updates retain minimap pixels without synchronous layout reads", async ({
  page,
}) => {
  await page.goto("/diagnostics.html");
  const result = await page.evaluate(async () => {
    const { initMinimapView } = await import("/navigation/view.js" as string);
    const panel = document.createElement("div");
    panel.innerHTML =
      '<svg width="256" height="256"><g id="minimap-view"><g id="minimap-entities"></g></g></svg><button id="minimap-zoom"></button>';
    document.body.append(panel);
    const svg = panel.querySelector("svg");
    if (!svg) throw new Error("Minimap SVG missing");
    const view = initMinimapView(svg, panel, () => {});
    view.update([512, 512], [256, 256]);
    await new Promise<void>((resolve) =>
      requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
    );
    let reads = 0;
    const bounds = svg.getBoundingClientRect.bind(svg);
    svg.getBoundingClientRect = () => {
      reads++;
      return bounds();
    };
    view.actors([0, 256, 256, 1, 0xff0000]);
    const first = panel.querySelector("#minimap-entities rect");
    if (!first) throw new Error("Initial actor dot missing");
    const before = first.outerHTML;
    view.actors([0, 320, 256, 1, 0xff0000]);
    const second = panel.querySelector("#minimap-entities rect");
    if (!second) throw new Error("Moved actor dot missing");
    const moved = second.outerHTML;
    view.actors([0, 320, 256, 1, 0xff0000]);
    const retained = panel.querySelector("#minimap-entities rect") === second;
    view.actors([]);
    return {
      reads,
      before,
      moved,
      retained,
      remaining: panel.querySelectorAll("rect").length,
    };
  });
  expect(result.moved).not.toBe(result.before);
  expect(result.retained).toBe(true);
  expect(result.remaining).toBe(0);
  expect(result.reads).toBe(0);
});

test("dense typed actor streams are copied without per-value iterator work and survive source reuse", async ({
  page,
}) => {
  await page.goto("/diagnostics.html");
  const result = await page.evaluate(async () => {
    const { initMinimapView } = await import("/navigation/view.js" as string);
    const panel = document.createElement("div");
    panel.innerHTML =
      '<svg width="256" height="256"><g id="minimap-view"><g id="minimap-entities"></g></g></svg><button id="minimap-zoom"></button>';
    document.body.append(panel);
    const svg = panel.querySelector("svg");
    if (!svg) throw new Error("Minimap SVG missing");
    const view = initMinimapView(svg, panel, () => {});
    view.update([512, 512], [256, 256]);
    const values = new Float64Array(20_000 * 5);
    for (let at = 0; at < values.length; at += 5)
      values.set([0, 256, 256, 1, 0xff0000], at);
    let iterations = 0;
    const original = values[Symbol.iterator].bind(values);
    Object.defineProperty(values, Symbol.iterator, {
      value: function* () {
        for (const value of original()) {
          iterations++;
          yield value;
        }
      },
    });
    view.actors(values);
    const copied = view.records();
    values.fill(-1); // Rust can overwrite/grow its memory as soon as the bridge returns.
    return {
      iterations,
      length: copied.length,
      first: Array.from(copied.slice(0, 5)),
      last: Array.from(copied.slice(-5)),
      count: panel.querySelector("rect")?.getAttribute("data-count"),
    };
  });
  expect(result.length).toBe(100_000);
  expect(result.first).toEqual([0, 256, 256, 1, 0xff0000]);
  expect(result.last).toEqual(result.first);
  expect(result.count).toBe("20000");
  expect(result.iterations).toBe(0);
});
