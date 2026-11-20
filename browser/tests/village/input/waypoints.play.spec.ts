import { expect } from "@playwright/test";
import { PNG } from "pngjs";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { centerVillage, freshHud } from "../fixtures/resources.js";

test("shift right-click walks successive destinations and clears authoritative markers", async ({
  page,
  request,
}, info) => {
  test.setTimeout(150_000);
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page, true);
  const renderer = info.project.name === "webgpu" ? "webgpu" : "webgl2";
  await page.goto(`/?play=1#zoom=.5&renderer=${renderer}`);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    renderer,
  );
  await centerVillage(page);
  const canvas = page.locator("#scene");
  const actors = async () =>
    JSON.parse((await canvas.getAttribute("data-actors")) ?? "[]") as {
      id: number;
      kind: string;
      x: number;
      y: number;
    }[];
  const scout = (await actors()).find((a) => a.kind === "Scout");
  if (!scout) throw new Error("Scout missing");
  await page.mouse.click(scout.x, scout.y);
  await expect(page.locator("#selection-name")).toContainText("Scout");
  const points = [
    { x: 900, y: 430 },
    { x: 450, y: 460 },
    { x: 950, y: 480 },
  ];
  // Record progress in-page: browser automation can pause across a short leg.
  // The sim replay separately pins exact visits; the browser checks ordered
  // consumption of the authoritative markers and the final snapped arrival.
  await page.evaluate(() => {
    const counts: number[] = [];
    Reflect.set(window, "waypointProgress", counts);
    new MutationObserver(() => {
      const count = document.querySelectorAll("#waypoint-markers g").length;
      if (counts.at(-1) !== count) counts.push(count);
    }).observe(document.body, { childList: true, subtree: true });
  });
  await page.mouse.click(900, 430, { button: "right" });
  await page.keyboard.down("Control");
  await page.mouse.click(450, 460, { button: "right" });
  await page.mouse.click(950, 480, { button: "right" });
  await page.keyboard.up("Control");
  await expect(
    page.locator(`#waypoint-markers g[data-unit="${scout.id}"]`),
  ).toHaveCount(3);
  const markers = page.locator(`#waypoint-markers g[data-unit="${scout.id}"]`);
  const targets = await markers.evaluateAll((elements) =>
    elements.map((el) => {
      const [x, y] = (el.getAttribute("transform") ?? "")
        .replace(/[^0-9., -]/g, "")
        .trim()
        .split(/[ ,]+/)
        .map(Number);
      return { x: x ?? NaN, y: y ?? NaN };
    }),
  );
  targets.forEach((target, index) => {
    const requested = points[index];
    expect(
      Math.hypot(
        target.x - (requested?.x ?? NaN),
        target.y - (requested?.y ?? NaN),
      ),
    ).toBeLessThan(18);
  });
  await page.mouse.move(1050, 200);
  await expect(page.locator("#waypoint-markers g")).toHaveCount(0, {
    timeout: 80_000,
  });
  const progress = await page.evaluate(
    () => Reflect.get(window, "waypointProgress") as number[],
  );
  expect(progress.slice(progress.indexOf(3))).toEqual([3, 2, 1, 0]);
  await freshHud(page);
  const final = (await actors()).find((a) => a.id === scout.id);
  const last = targets.at(-1);
  expect(
    Math.hypot(
      (final?.x ?? NaN) - (last?.x ?? NaN),
      (final?.y ?? NaN) - (last?.y ?? NaN),
    ),
  ).toBeLessThan(18);
  await page.keyboard.down("Control");
  await page.mouse.click(450, 460, { button: "right" });
  await page.keyboard.up("Control");
  await expect(page.locator("#waypoint-markers g")).toHaveCount(1);
  await page.locator("#stop-units").click();
  await expect(page.locator("#waypoint-markers g")).toHaveCount(0);
});

test("live attack-move legs stay visible after placement and Stop clears them", async ({
  page,
  request,
}, info) => {
  test.setTimeout(90_000);
  expect(
    (await request.post("/maps/reset?scenario=general-army")).status(),
  ).toBe(204);
  await gameAssets(page, true);
  const renderer = info.project.name === "webgpu" ? "webgpu" : "webgl2";
  await page.goto(`/?play=1#x=109&y=133&zoom=.5&renderer=${renderer}`);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  const scene = page.locator("#scene");
  await expect(scene).toHaveAttribute("data-actors", /"kind":"Militia"/);
  await page.mouse.move(615, 344);
  await page.mouse.down();
  await page.mouse.move(852, 485, { steps: 5 });
  await page.mouse.up();
  await expect(page.locator("#selection-name")).toHaveText("Militia x4");
  const ringPixels = async () => {
    const { data } = PNG.sync.read(
      await page.screenshot({
        clip: { x: 600, y: 155, width: 480, height: 340 },
      }),
    );
    let count = 0;
    for (let i = 0; i < data.length; i += 4) {
      const [r, g, b] = [data[i] ?? 0, data[i + 1] ?? 0, data[i + 2] ?? 0];
      if (r >= 220 && g >= 185 && b < 130) count++;
    }
    return count;
  };
  await expect.poll(ringPixels).toBeGreaterThan(20);

  const attack = page.locator("#attack-move-units");
  await attack.click();
  await page.keyboard.down("Shift");
  await page.mouse.click(1010, 173, { button: "right" });
  await page.mouse.click(1125, 385, { button: "right" });
  await page.keyboard.up("Shift");
  await expect(scene).toHaveAttribute("data-order-mode", "");
  const markers = page.locator("#waypoint-markers g[data-order=attack-move]");
  await expect(markers).toHaveCount(2);
  const legs = page.locator('#waypoint-markers path[data-order="attack-move"]');
  await expect(legs).toHaveCount(2);
  await expect(legs.first()).toHaveAttribute("d", /^M[^L]+L[^L]+$/);

  await attack.click();
  await page.keyboard.press("Escape");
  await expect(scene).toHaveAttribute("data-order-mode", "");
  await expect(markers).toHaveCount(2);
  await page.locator("#stop-units").click();
  await expect(markers).toHaveCount(0);

  await attack.click();
  await page.mouse.click(466, 188);
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "0");
  await expect(scene).toHaveAttribute("data-order-mode", "");
  await expect(scene).not.toHaveAttribute("data-actors", /"selected":true/);
  expect(await ringPixels()).toBe(0);
});

test("panning 200 unchanged queues retains every waypoint subtree", async ({
  page,
}) => {
  await page.route("**/start/bootstrap.js", (route) =>
    route.fulfill({
      contentType: "application/javascript",
      body: `import '/interface/details.js';`,
    }),
  );
  await page.goto("/?play=1");
  const result = await page.evaluate(() => {
    const present = Reflect.get(window, "aoeWaypoints") as (
      v: Float64Array,
    ) => void;
    const values = new Float64Array(200 * 17 * 6);
    for (let unit = 0; unit < 200; unit++) {
      values.set([unit, 0, -100, unit * 20, 0, 0], unit * 17 * 6);
      for (let index = 0; index < 16; index++) {
        values.set(
          [unit, index + 1, index * 100, unit * 20, Number(index >= 8), 0],
          (unit * 17 + index + 1) * 6,
        );
      }
    }
    present(values);
    const layer = document.getElementById("waypoint-markers");
    if (!layer) throw new Error("waypoint layer missing");
    const before = Array.from(layer.querySelectorAll("*"));
    const observer = new MutationObserver(() => {});
    observer.observe(layer, { childList: true, subtree: true });
    for (let frame = 0; frame < 10; frame++) {
      for (let i = 0; i < values.length; i += 6) {
        values[i + 2] = (values[i + 2] ?? NaN) - 2;
        values[i + 3] = (values[i + 3] ?? NaN) + 1;
      }
      present(values);
    }
    const after = Array.from(layer.querySelectorAll("*"));
    const changes = observer.takeRecords().length;
    observer.disconnect();
    const transform = layer.querySelector("g")?.getAttribute("transform");
    const lines = Array.from(layer.querySelectorAll("path[data-order]"))
      .slice(0, 2)
      .map((line) => ({
        order: line.getAttribute("data-order"),
        d: line.getAttribute("d"),
        stroke: getComputedStyle(line).stroke,
        dash: getComputedStyle(line).strokeDasharray,
      }));
    const flags = Array.from(layer.querySelectorAll("g[data-unit='0']")).map(
      (flag) => ({
        order: flag.getAttribute("data-order"),
        fill: getComputedStyle(flag).fill,
        shape: flag.querySelector("path")?.getAttribute("d"),
      }),
    );
    const first = layer.querySelector("g");
    const glyph = first?.querySelector("path");
    values[10] = 1;
    present(values);
    const changedLine = layer
      .querySelector('path[data-order="attack-move"]')
      ?.getAttribute("d");
    const changedShape = glyph?.getAttribute("d");
    const reusedGlyph = first?.querySelector("path") === glyph;
    present(new Float64Array());
    return {
      changedLine,
      changedShape,
      reusedGlyph,
      count: before.length,
      retained: before.every((node, i) => node === after[i]),
      changes,
      transform,
      lines,
      flags,
      remaining: layer.children.length,
    };
  });
  expect(result.count).toBe(13_200);
  expect(result.retained).toBe(true);
  expect(result.changes).toBe(0);
  expect(result.transform).toBe("translate(-20 10)");
  expect(result.lines.map((line) => line.order)).toEqual([
    "move",
    "attack-move",
  ]);
  expect(result.lines[0]?.d).toMatch(/^M-120,10L-20,10/);
  expect(result.lines[0]?.d).toContain("M-20,10L80,10");
  expect(result.lines[1]?.d).toContain("680,10");
  expect(result.lines[0]?.d).not.toContain("780,10");
  expect(result.lines[1]?.d).toContain("780,10");
  expect(result.lines[0]?.stroke).not.toBe(result.lines[1]?.stroke);
  expect(result.flags.map((flag) => flag.order)).toEqual([
    ...Array<string>(8).fill("move"),
    ...Array<string>(8).fill("attack-move"),
  ]);
  expect(result.flags[0]?.fill).not.toBe(result.flags[8]?.fill);
  expect(result.flags[0]?.shape).toBeTruthy();
  expect(result.flags[8]?.shape).toBeTruthy();
  expect(result.flags[0]?.shape).not.toBe(result.flags[8]?.shape);
  expect(result.changedLine).toMatch(/^M-120,10L-20,10/);
  expect(result.changedShape).toBe(result.flags[8]?.shape);
  expect(result.reusedGlyph).toBe(true);
  expect(result.lines[0]?.dash).not.toBe("none");
  expect(result.lines[1]?.dash).toBe("none");
  expect(result.remaining).toBe(0);
});

test("shared move and attack-move orders draw one line per leg from the group centre", async ({
  page,
}) => {
  await page.route("**/start/bootstrap.js", (route) =>
    route.fulfill({
      contentType: "application/javascript",
      body: "import '/interface/details.js';",
    }),
  );
  await page.goto("/?play=1");
  await page.evaluate(() => {
    const present = Reflect.get(window, "aoeWaypoints") as (
      v: Float64Array,
    ) => void;
    present(
      new Float64Array([
        1, 0, 20, 10, 0, 0, 1, 1, 200, 200, 0, 31, 1, 2, 400, 400, 1, 32, 2, 0,
        30, 20, 0, 0, 2, 1, 205, 197, 0, 31, 2, 2, 400, 400, 1, 32, 3, 0, 50,
        60, 0, 0, 3, 1, 200, 200, 0, 33,
      ]),
    );
  });
  const layer = page.locator("#waypoint-markers");
  await expect(layer.locator("g")).toHaveText(["1", "2", "1"]);
  expect(
    await layer
      .locator("path[data-order=move]")
      .evaluateAll((lines) => lines.map((line) => line.getAttribute("d"))),
  ).toEqual(["M25,15L200,200", "M50,60L200,200"]);
  expect(
    await layer
      .locator("path[data-order=attack-move]")
      .evaluateAll((lines) => lines.map((line) => line.getAttribute("d"))),
  ).toEqual(["M200,200L400,400"]);
});
