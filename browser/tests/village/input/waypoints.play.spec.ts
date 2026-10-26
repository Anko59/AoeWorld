import { expect } from "@playwright/test";
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
  await page.keyboard.down("Shift");
  await page.mouse.click(450, 460, { button: "right" });
  await page.mouse.click(950, 480, { button: "right" });
  await page.keyboard.up("Shift");
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
  await page.keyboard.down("Shift");
  await page.mouse.click(450, 460, { button: "right" });
  await page.keyboard.up("Shift");
  await expect(page.locator("#waypoint-markers g")).toHaveCount(1);
  await page.locator("#stop-units").click();
  await expect(page.locator("#waypoint-markers g")).toHaveCount(0);
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
    const values = new Float64Array(200 * 16 * 4);
    for (let unit = 0; unit < 200; unit++) {
      for (let index = 0; index < 16; index++) {
        values.set(
          [unit, index + 1, index * 100, unit * 20],
          (unit * 16 + index) * 4,
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
      for (let i = 0; i < values.length; i += 4) {
        values[i + 2] = (values[i + 2] ?? NaN) - 2;
        values[i + 3] = (values[i + 3] ?? NaN) + 1;
      }
      present(values);
    }
    const after = Array.from(layer.querySelectorAll("*"));
    const changes = observer.takeRecords().length;
    observer.disconnect();
    const transform = layer.querySelector("g")?.getAttribute("transform");
    const points = layer.querySelector("polyline")?.getAttribute("points");
    present(new Float64Array());
    return {
      count: before.length,
      retained: before.every((node, i) => node === after[i]),
      changes,
      transform,
      points,
      remaining: layer.children.length,
    };
  });
  expect(result.count).toBe(13_000);
  expect(result.retained).toBe(true);
  expect(result.changes).toBe(0);
  expect(result.transform).toBe("translate(-20 10)");
  expect(result.points).toContain("-20,10");
  expect(result.remaining).toBe(0);
});
