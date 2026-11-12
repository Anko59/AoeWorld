import { expect } from "@playwright/test";
import { test } from "../../server.js";

test("dense holdings retain an owned snapshot without boxing a numeric signature", async ({
  page,
}) => {
  await page.goto("/diagnostics.html");
  const result = await page.evaluate(async () => {
    const { initHoldings } = await import(
      "/navigation/landscape/holdings.js" as string
    );
    const layer = initHoldings();
    const values = new Float64Array(20_000 * 4);
    for (let at = 0; at < values.length; at += 4)
      values.set([0, 0, 2, 0xff0000], at);
    const same = values.slice();
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
    const first = layer.update(values);
    const canvas = document.createElement("canvas"),
      context = canvas.getContext("2d");
    if (!context) throw new Error("Canvas context missing");
    const count = layer.draw(context, () => {}, 1, true);
    values.fill(-1); // The bridge may reuse its memory after update returns.
    const retained = !layer.update(same);
    same[2] = 3;
    const changed = layer.update(same);
    layer.reset();
    const reset = layer.update(same);
    const invalid = new Float64Array([NaN, 0, 2, 0xff0000]);
    layer.update(invalid);
    const nanRetained = !layer.update(invalid.slice());
    layer.update(new Float64Array([-0, 0, 2, 0xff0000]));
    const zeroRetained = !layer.update(new Float64Array([0, 0, 2, 0xff0000]));
    return {
      first,
      count,
      retained,
      changed,
      reset,
      iterations,
      nanRetained,
      zeroRetained,
    };
  });
  expect(result.first).toBe(true);
  expect(result.count).toBe(20_000);
  expect(result.retained).toBe(true);
  expect(result.changed).toBe(true);
  expect(result.reset).toBe(true);
  expect(result.nanRetained).toBe(true);
  expect(result.zeroRetained).toBe(true);
  expect(result.iterations).toBe(0);
});
