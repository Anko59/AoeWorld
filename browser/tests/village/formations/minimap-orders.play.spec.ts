import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { centerVillage, freshHud } from "../fixtures/resources.js";
import {
  type Point,
  armOrder,
  army,
  camera,
  expectAt,
  lastPing,
  mapBox,
  nativeFrame,
  outside,
  pixel,
  press,
  recordPings,
  shown,
} from "./minimap.js";

const right = { button: "right" } as const;
const soldiers = [2, 3, 4, 5, 6];
const [move, rally, queuedMove, formation, general, patrol] = [
  2, 20, 21, 29, 32, 46,
];

for (const layout of ["letterboxed", "native", "overview"] as const) {
  test(`a right-click on the ${layout} map sends the selection there and from outside it nowhere`, async ({
    page,
  }) => {
    test.setTimeout(60_000);
    const orders = await army(page);
    if (layout === "native") await nativeFrame(page);
    if (layout === "overview") {
      await page.keyboard.press("m");
      await expect(page.locator("#minimap")).toHaveClass(/expanded/);
    }
    await recordPings(page);
    const before = await camera(page);
    const first = await pixel(page, { x: 12_000, y: 5_000 });
    await page.mouse.click(first.x, first.y, right);
    await expect.poll(() => orders.length).toBe(1);
    expect(orders[0]).toMatchObject({
      tag: formation,
      ids: soldiers,
      flags: [0, 0],
    });
    await expectAt(page, orders[0], first);
    await expect.poll(async () => (await lastPing(page))?.kind).toBe("move");
    const ping = await lastPing(page);
    expect(
      Math.hypot((ping?.x ?? NaN) - first.x, (ping?.y ?? NaN) - first.y),
    ).toBeLessThan(6);

    // A clamped point would march the army to the border of the world.
    const beyond = await outside(page, layout === "native");
    await press(page, beyond);
    // The native frame's clip path keeps a real pointer off its corners.
    if (layout !== "native")
      await page.mouse.click(beyond[0].x, beyond[0].y, right);
    const second = await pixel(page, { x: 3_000, y: 9_000 });
    await page.mouse.click(second.x, second.y, right);
    await expect.poll(() => orders.length).toBeGreaterThan(1);
    expect(orders).toHaveLength(2);
    await expectAt(page, orders[1], second);
    expect(await camera(page)).toBe(before);
  });
}

test("the ordered point follows the minimap's own zoom and pan up to the corner of the world", async ({
  page,
}) => {
  test.setTimeout(60_000);
  const orders = await army(page);
  const panel = page.locator("#minimap");
  await page.evaluate(() => {
    location.hash = "x=2&y=2&zoom=1";
  });
  await expect(panel).toHaveAttribute("data-camera-x", "2");
  const box = await mapBox(page);
  const x = Math.round(box.x + box.width / 2);
  const y = Math.round(box.y + box.height / 2);
  await page.mouse.move(x, y - 30);
  await page.mouse.wheel(0, -1_800);
  await expect
    .poll(async () => Number(await panel.getAttribute("data-minimap-zoom")))
    .toBeGreaterThan(4);
  const view = page.locator("#minimap-view");
  const followed = await view.getAttribute("transform");
  await page.mouse.down();
  await page.mouse.move(x, y + 30, { steps: 5 });
  await page.mouse.up();
  await expect(view).not.toHaveAttribute("transform", followed ?? "");
  // The pan stops at the world's first corner, now in the middle of the map.
  const corner = await shown(page, { x: 0, y: 0 });
  expect(Math.hypot(corner.x - x, corner.y - y)).toBeLessThan(2);
  const before = await camera(page);
  for (const [dx, dy] of [
    [0, -12],
    [12, 0],
    [-12, 0],
  ] as const)
    await page.mouse.click(x + dx, y + dy, right);
  await page.mouse.click(x, y + 12, right);
  await expect.poll(() => orders.length).toBe(1);
  expect(orders[0]).toMatchObject({ tag: formation, ids: soldiers });
  await expectAt(page, orders[0], { x, y: y + 12 });
  expect(await camera(page)).toBe(before);
});

test("queue keys and armed orders work on the map and an unarmed left click still only jumps", async ({
  page,
}, info) => {
  test.setTimeout(90_000);
  const orders = await army(page);
  const scene = page.locator("#scene");
  const at = await pixel(page, { x: 11_000, y: 7_000 });
  const click = async (button: "left" | "right") => {
    const count = orders.length;
    await page.mouse.click(at.x, at.y, { button });
    await expect.poll(() => orders.length).toBe(count + 1);
    await expectAt(page, orders[count], at);
    return orders[count];
  };
  const arm = (id: string, mode: string) => armOrder(page, id, mode);
  await recordPings(page);
  const before = await camera(page);

  // Firefox opens its own menu on Shift; Ctrl is the other queue key.
  for (const key of info.project.name.startsWith("firefox")
    ? ["Control"]
    : ["Control", "Shift"]) {
    await page.keyboard.down(key);
    const queued = await click("right");
    await page.keyboard.up(key);
    expect(queued).toMatchObject({ tag: formation, flags: [1, 0] });
  }
  // Two presses between two frames are two waypoints, in their order.
  const far = await pixel(page, { x: 5_000, y: 9_000 });
  const single = orders.length;
  await press(page, [far, at], { ctrlKey: true });
  await expect.poll(() => orders.length).toBe(single + 2);
  await expectAt(page, orders[single], far);
  await expectAt(page, orders[single + 1], at);

  for (const button of ["right", "left"] as const) {
    await arm("#attack-move-units", "attack-move");
    expect(await click(button)).toMatchObject({
      tag: formation,
      flags: [0, 1],
    });
    await expect(scene).toHaveAttribute("data-order-mode", "");
    await arm("#patrol-units", "patrol");
    expect(await click(button)).toMatchObject({ tag: patrol, ids: soldiers });
    expect((await lastPing(page))?.kind).toBe("patrol");
    await expect(scene).toHaveAttribute("data-order-mode", "");
  }
  await arm("#attack-move-units", "attack-move");
  await page.keyboard.down("Control");
  expect(await click("left")).toMatchObject({ tag: formation, flags: [1, 1] });
  await expect(scene).toHaveAttribute("data-order-mode", "attack-move");
  await page.keyboard.up("Control");
  await expect(scene).toHaveAttribute("data-order-mode", "");
  await arm("#move-units", "move");
  expect(await click("left")).toMatchObject({ tag: formation, flags: [0, 0] });
  await expect(scene).toHaveAttribute("data-order-mode", "");
  expect(await camera(page)).toBe(before);

  // Nothing is armed any more: the left button only moves the camera.
  const total = orders.length;
  await page.mouse.click(at.x, at.y);
  await expect.poll(() => camera(page)).not.toBe(before);
  expect(await click("right")).toMatchObject({ tag: formation, flags: [0, 0] });
  expect(orders).toHaveLength(total + 1);
});

test("the browser menu never opens on the map and the map's buttons keep theirs", async ({
  page,
}) => {
  test.setTimeout(60_000);
  await army(page);
  const expected = {
    "#minimap-map": true,
    "#minimap-frame rect": true,
    "#minimap-zoom": false,
    "#recenter": false,
    "#overview-toggle": false,
  };
  const prevented = () =>
    page.evaluate(
      (selectors) =>
        Object.fromEntries(
          selectors.map((selector) => {
            const node = document.querySelector(selector);
            if (!node) throw new Error(`${selector} is missing`);
            const event = new MouseEvent("contextmenu", {
              bubbles: true,
              cancelable: true,
            });
            return [selector, !node.dispatchEvent(event)];
          }),
        ),
      Object.keys(expected),
    );
  expect(await prevented()).toEqual(expected);
  await page.keyboard.press("m");
  await expect(page.locator("#minimap")).toHaveClass(/expanded/);
  expect(await prevented()).toEqual(expected);
});

test("a map click rallies a selected building on the ground and does nothing without a selection", async ({
  page,
}) => {
  test.setTimeout(60_000);
  const orders = await army(page);
  await recordPings(page);
  const selection = page.locator("#selection");
  const at = await pixel(page, { x: 11_000, y: 7_000 });
  await page.keyboard.press("Escape");
  await expect(selection).toHaveAttribute("data-count", "0");
  await page.mouse.click(at.x, at.y, right);
  await page.keyboard.press("h");
  await expect(selection).toHaveAttribute("data-kind", "TownCenter");
  expect(await lastPing(page)).toBeUndefined();
  const before = await camera(page);
  await page.mouse.click(at.x, at.y, right);
  await expect.poll(() => orders.length).toBe(1);
  // The fixture's Town Center has id 1; target variant 0 is the ground.
  expect(orders[0]).toMatchObject({ tag: rally, ids: [1], extra: [0] });
  await expectAt(page, orders[0], at);
  expect(await lastPing(page)).toBeUndefined();
  expect(await camera(page)).toBe(before);
});

test("without an owned unit a left click disarms and jumps and a right click disarms and rallies", async ({
  page,
}) => {
  test.setTimeout(60_000);
  const orders = await army(page);
  const scene = page.locator("#scene");
  const selection = page.locator("#selection");
  const at = await pixel(page, { x: 11_000, y: 7_000 });
  const leftOver = async () => {
    await armOrder(page, "#attack-move-units", "attack-move");
    await page.keyboard.press("h");
    await expect(selection).toHaveAttribute("data-kind", "TownCenter");
  };
  await page.keyboard.press("Control+1");
  const before = await camera(page);
  // Outside the world an armed left press neither orders nor jumps.
  await armOrder(page, "#attack-move-units", "attack-move");
  const [beyond] = await outside(page, false);
  await page.mouse.click(beyond.x, beyond.y);
  await freshHud(page);
  expect(await camera(page)).toBe(before);

  await leftOver();
  await page.mouse.click(at.x, at.y);
  await expect.poll(() => camera(page)).not.toBe(before);
  await expect(scene).toHaveAttribute("data-order-mode", "");

  await page.keyboard.press("1");
  await expect(selection).toHaveAttribute("data-count", "5");
  await leftOver();
  await page.mouse.click(at.x, at.y, right);
  await expect.poll(() => orders.length).toBeGreaterThan(0);
  expect(orders).toHaveLength(1);
  expect(orders[0]).toMatchObject({ tag: rally, ids: [1], extra: [0] });
  await expect(scene).toHaveAttribute("data-order-mode", "");
});

test("a right-click on the map cancels a far-view production destination and orders nothing", async ({
  page,
}) => {
  test.setTimeout(90_000);
  const orders = await army(page);
  await page.evaluate(() => {
    location.hash = "x=8192.5&y=8192.5&zoom=.04";
  });
  const html = page.locator("html");
  await expect(html).toHaveAttribute("data-lod", /^(map|markers)$/);
  // The first far view slides the minimap in; measure it afterwards.
  await page.waitForFunction(
    () => sessionStorage.getItem("aoeworld.cockpit-reveal") !== null,
  );
  await expect(html).not.toHaveAttribute("data-cockpit-reveal", /.+/, {
    timeout: 15_000,
  });
  const destination = page.locator("#units-here-destination");
  await expect(destination).toBeEnabled();
  await destination.evaluate((button) => (button as HTMLButtonElement).click());
  await expect(destination).toHaveAttribute("aria-pressed", "true");
  const at = await pixel(page, { x: 11_000, y: 7_000 });
  await page.mouse.click(at.x, at.y, right);
  await expect(destination).toHaveAttribute("aria-pressed", "false");
  await expect(page.locator("#units-here-status")).toHaveText(
    "Order cancelled.",
  );
  // The selection is kept, so the next press is an order: the only one.
  await page.mouse.click(at.x, at.y, right);
  await expect.poll(() => orders.length).toBeGreaterThan(0);
  expect(orders).toHaveLength(1);
  await expectAt(page, orders[0], at);
});

test("at most eight map pings are drawn and the oldest go first", async ({
  page,
}) => {
  test.setTimeout(60_000);
  await army(page);
  const points: Point[] = [];
  for (let x = 1_500; points.length < 10; x += 1_300)
    points.push(await pixel(page, { x, y: 8_000 }));
  // Counted in the task that draws them: a ping is gone in a moment.
  const drawn = await page.evaluate((points) => {
    for (const { x, y } of points)
      document.getElementById("minimap-map")?.dispatchEvent(
        new PointerEvent("pointerdown", {
          bubbles: true,
          cancelable: true,
          button: 2,
          ctrlKey: true,
          clientX: x,
          clientY: y,
        }),
      );
    const pings = Array.from(
      document.querySelectorAll("[data-minimap-ping]"),
      (node) => {
        const box = node.getBoundingClientRect();
        return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
      },
    );
    const near = (a: Point, b: Point) => Math.hypot(a.x - b.x, a.y - b.y) < 3;
    return points.map(
      (point) => pings.filter((ping) => near(ping, point)).length,
    );
  }, points);
  expect(drawn).toEqual([0, 0, 1, 1, 1, 1, 1, 1, 1, 1]);
});

test("Alt on the map moves a selected General without his army", async ({
  page,
}) => {
  test.setTimeout(60_000);
  const orders = await army(page, "General", 23, false);
  const at = await pixel(page, { x: 11_000, y: 7_000 });
  await press(page, [at], { altKey: true });
  await expect.poll(() => orders.length).toBe(1);
  expect(orders[0]).toMatchObject({ tag: general, extra: [0], flags: [] });
  await expectAt(page, orders[0], at);
  await page.mouse.click(at.x, at.y, right);
  await expect.poll(() => orders.length).toBe(2);
  expect(orders[1]).toMatchObject({
    tag: move,
    ids: orders[0]?.ids,
    flags: [0],
  });
  await press(page, [at], { altKey: true, ctrlKey: true });
  await expect.poll(() => orders.length).toBe(3);
  expect(orders[2]).toMatchObject({ tag: queuedMove, ids: orders[0]?.ids });
});

test("a map ping names its order, ends by itself and is cleared by Stop", async ({
  page,
}) => {
  test.setTimeout(60_000);
  await army(page);
  await recordPings(page);
  const at = await pixel(page, { x: 11_000, y: 7_000 });
  await page.mouse.click(at.x, at.y, right);
  await expect.poll(async () => (await lastPing(page))?.kind).toBe("move");
  await expect(page.locator("[data-minimap-ping]")).toHaveCount(0, {
    timeout: 3_000,
  });
  await page
    .locator("#attack-move-units")
    .evaluate((button) => (button as HTMLButtonElement).click());
  // Stop in the same task as the ping, however slow this machine draws.
  await page.evaluate(() => {
    new MutationObserver((_records, observer) => {
      const pings = () => document.querySelectorAll("[data-minimap-ping]");
      const kind = pings()[0]?.getAttribute("data-minimap-ping");
      if (kind === undefined) return;
      observer.disconnect();
      document.getElementById("stop-units")?.click();
      Reflect.set(window, "stoppedPings", [kind, pings().length]);
    }).observe(document.body, {
      subtree: true,
      childList: true,
      attributes: true,
      attributeFilter: ["data-minimap-ping"],
    });
  });
  await page.mouse.click(at.x, at.y, right);
  await expect
    .poll(() => page.evaluate(() => Reflect.get(window, "stoppedPings")))
    .toEqual(["attack-move", 0]);
});

// The real server, not a frozen snapshot: it streams the units a player owns
// outside the subscribed view too, so the order must reach them.
test("a unit left far behind the camera takes the order and the server plans it", async ({
  page,
  request,
}, info) => {
  test.setTimeout(120_000);
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page, true);
  const renderer = info.project.name === "webgpu" ? "webgpu" : "webgl2";
  await page.goto(`/?play=1#zoom=.5&renderer=${renderer}`);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await centerVillage(page);
  const scene = page.locator("#scene");
  const scout = (
    JSON.parse((await scene.getAttribute("data-actors")) ?? "[]") as {
      id: number;
      kind: string;
      x: number;
      y: number;
    }[]
  ).find((actor) => actor.kind === "Scout");
  if (!scout) throw new Error("Scout missing");
  await page.mouse.click(scout.x, scout.y);
  await expect(page.locator("#selection-name")).toContainText("Scout");

  // At this zoom the view is about forty tiles wide.
  const panel = page.locator("#minimap");
  const away = await panel.evaluate((map) => {
    const x = Number(map.dataset.cameraX) + 120;
    location.hash = `x=${x}&y=${map.dataset.cameraY}&zoom=.5`;
    return String(x);
  });
  await expect(panel).toHaveAttribute("data-camera-x", away);
  await expect(scene).toHaveAttribute("data-view-painted", "true", {
    timeout: 30_000,
  });
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "1");

  const box = await mapBox(page);
  const x = Math.round(box.x + box.width / 2);
  const y = Math.round(box.y + box.height / 2);
  await page.mouse.move(x, y);
  await page.mouse.wheel(0, -1_800);
  await expect
    .poll(async () => Number(await panel.getAttribute("data-minimap-zoom")))
    .toBeGreaterThan(4);
  const before = await camera(page);
  await page.keyboard.down("Control");
  await page.mouse.click(x + 4, y, right);
  await page.keyboard.up("Control");
  // Only the server's own queue draws this marker.
  await expect(
    page.locator(`#waypoint-markers g[data-unit="${scout.id}"]`),
  ).toHaveCount(1);
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "1");
  expect(await camera(page)).toBe(before);
});
