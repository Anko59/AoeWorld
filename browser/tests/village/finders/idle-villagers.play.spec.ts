import { expect, type Page, type WebSocketRoute } from "@playwright/test";
import { test } from "../../../server.js";
import { inputFixture, position } from "../input/fixture.js";
import { villagerCombat } from "../../wire/combat.js";

async function selected(page: Page) {
  return page.locator("#scene").evaluate((canvas) =>
    (
      JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as {
        id: number;
        selected: boolean;
      }[]
    )
      .filter((actor) => actor.selected)
      .map((actor) => actor.id),
  );
}

test("idle finders and minimap zoom remain separate clickable controls", async ({
  page,
}) => {
  await inputFixture(page, "webgpu");
  for (const width of [1280, 950]) {
    await page.setViewportSize({ width, height: 768 });
    for (const expanded of [false, true]) {
      if (expanded) await page.locator("#overview-toggle").click();
      const selectors = [
        "#idle-villagers",
        "#idle-military",
        "#minimap-zoom",
        "#minimap-map",
      ];
      const boxes = [];
      for (const selector of selectors) {
        await expect(page.locator(selector)).toBeVisible();
        const box = await page.locator(selector).boundingBox();
        if (!box) throw new Error(`${selector} has no clickable bounds`);
        expect(box.x).toBeGreaterThanOrEqual(0);
        expect(box.y).toBeGreaterThanOrEqual(0);
        expect(box.x + box.width).toBeLessThanOrEqual(width);
        expect(box.y + box.height).toBeLessThanOrEqual(768);
        boxes.push(box);
      }
      for (const [i, a] of boxes.entries())
        for (const [offset, b] of boxes.slice(i + 1).entries()) {
          const j = i + offset + 1;
          const overlap =
            Math.min(a.x + a.width, b.x + b.width) > Math.max(a.x, b.x) &&
            Math.min(a.y + a.height, b.y + b.height) > Math.max(a.y, b.y);
          expect(
            overlap,
            `${selectors[i]} overlaps ${selectors[j]} at ${width}px`,
          ).toBe(false);
        }
      if (expanded) await page.locator("#minimap-close").click();
    }
  }
});

test("period and the idle villager counter cycle owned workers and centre the camera", async ({
  page,
}, info) => {
  const renderer = info.project.name === "webgpu" ? "webgpu" : "webgl2";
  await inputFixture(page, renderer);
  await page.mouse.move(640, 360);
  await expect(page.locator("#idle-villager-count")).toHaveText("2");
  // Idle workers are flagged on the map and the finder is highlighted.
  await expect(page.locator("#idle-villagers")).toHaveClass(/has-idle/);
  await expect(page.locator("#idle-markers .idle-marker")).toHaveCount(2);
  for (const [id, x, y, button] of [
    [2, 8194.5, 8194.5, false],
    [3, 8196.5, 8194.5, true],
    [2, 8194.5, 8194.5, false],
  ] as const) {
    if (button) await page.locator("#idle-villagers").click();
    else await page.keyboard.press(".");
    await expect.poll(() => selected(page)).toEqual([id]);
    for (const [axis, value] of [
      ["x", x],
      ["y", y],
    ] as const)
      await expect
        .poll(async () =>
          Number(
            await page.locator("#minimap").getAttribute(`data-camera-${axis}`),
          ),
        )
        .toBeCloseTo(value, 1);
  }
  // Text entry and an open menu keep ordinary typing out of gameplay.
  await page.evaluate(() => {
    const input = document.createElement("input");
    input.id = "finder-text";
    document.body.append(input);
    input.focus();
  });
  await page.keyboard.press(".");
  await expect(page.locator("#finder-text")).toHaveValue(".");
  await expect.poll(() => selected(page)).toEqual([2]);
  await page.locator("#menu-toggle").click();
  await page.keyboard.press(".");
  await expect.poll(() => selected(page)).toEqual([2]);
});

test("idle counter follows task deltas and removals and does nothing when empty", async ({
  page,
}, info) => {
  let socket: WebSocketRoute | undefined;
  let revision: number[] = [];
  await inputFixture(
    page,
    info.project.name === "webgpu" ? "webgpu" : "webgl2",
    [],
    false,
    0,
    (bytes, route) => {
      socket = route;
      if (bytes[0] !== 1) return;
      revision = [];
      for (const byte of bytes.subarray(1)) {
        revision.push(byte);
        if (byte < 128) break;
      }
    },
  );
  await page.mouse.move(640, 360);
  await expect(page.locator("#idle-villager-count")).toHaveText("2");
  if (!socket) throw new Error("Game socket missing");
  // Authoritative delta: worker 2 starts moving, then worker 3 disappears.
  socket.send(
    Buffer.from([
      2,
      ...revision,
      1,
      1,
      2,
      0,
      ...position(8194.5, 8194.5),
      1,
      0,
      0,
      0,
      1,
      0,
      0,
      ...villagerCombat,
      0,
      0,
      0,
      0,
    ]),
  );
  await expect(page.locator("#idle-villager-count")).toHaveText("1");
  socket.send(Buffer.from([2, ...revision, 2, 0, 1, 3, 0, 0, 0]));
  await expect(page.locator("#idle-villager-count")).toHaveText("0");
  await expect(page.locator("#idle-villagers")).toBeDisabled();
  const before = await selected(page);
  await page.keyboard.press(".");
  await expect.poll(() => selected(page)).toEqual(before);
  // Reconnecting replaces the index from the next complete snapshot.
  await page.reload();
  await expect(page.locator("#idle-villager-count")).toHaveText("2");
});
