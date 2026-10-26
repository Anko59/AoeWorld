import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { building, inputFixture, screen, selectWorker } from "./fixture.js";

test("a nonzero player keeps ownership after its primary scout enters a Castle", async ({
  page,
}, info) => {
  test.setTimeout(90_000);
  const castle = building(20, 16, 8197, 8188, 4, 900, true, 1);
  let revision: number[] = [1];
  await inputFixture(
    page,
    info.project.name === "webgpu" ? "webgpu" : "webgl2",
    [castle],
    false,
    1,
    (bytes, socket) => {
      if (bytes[0] === 1) {
        revision = [];
        for (const byte of bytes.subarray(1)) {
          revision.push(byte);
          if (byte < 128) break;
        }
      }
      if (bytes[0] === 25)
        socket.send(
          Buffer.from([
            2,
            ...revision,
            1,
            0,
            1,
            0,
            0,
            1,
            ...castle.slice(0, -1),
            1,
            0,
            1,
            45,
            45,
            0,
          ]),
        );
    },
  );
  await page.mouse.click(640, 360);
  await expect(page.locator("#selection-name")).toContainText("Scout");
  await page.locator("#garrison").click();
  const center = screen(8199, 8190);
  await page.mouse.click(center.x, center.y);
  await page.mouse.click(center.x, center.y);
  await expect(page.locator("#selection-name")).toHaveText("Castle");
  await expect(page.locator("#selection")).toHaveAttribute(
    "data-owned",
    "true",
  );
  await expect(page.locator("#garrison-count")).toHaveText("1 / 20");
  await expect(page.locator("#ungarrison")).toBeEnabled();
});

test("an enemy inside a friendly shelter keeps the attack cursor and order", async ({
  page,
}, info) => {
  test.setTimeout(90_000);
  const commands = await inputFixture(
    page,
    info.project.name === "webgpu" ? "webgpu" : "webgl2",
    [building(20, 0, 8188, 8191, 4)],
    true,
  );
  await selectWorker(page);
  const enemy = screen(8189.5, 8192.5);
  await page.mouse.move(enemy.x, enemy.y);
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-command-cursor",
    "attack",
  );
  await page.mouse.click(enemy.x, enemy.y, { button: "right" });
  await expect.poll(() => commands.at(-1)?.[0]).toBe(7);
  expect(commands.at(-1)?.at(-1)).toBe(4);
});

test("a full shelter shows its capacity and receives a garrison order", async ({
  page,
}, info) => {
  test.setTimeout(90_000);
  const full = [
    ...building(20, 0, 8197, 8188, 4).slice(0, -1),
    15,
    ...Array.from({ length: 15 }, (_, i) => [40 + i, 0, 25, 25]).flat(),
  ];
  const commands = await inputFixture(
    page,
    info.project.name === "webgpu" ? "webgpu" : "webgl2",
    [full],
  );
  const center = screen(8199, 8190);
  await page.mouse.click(center.x, center.y);
  await expect(page.locator("#garrison-count")).toHaveText("15 / 15");
  await expect(page.locator("#garrison-units button")).toHaveCount(15);
  await selectWorker(page);
  await page.mouse.move(center.x, center.y);
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-command-cursor",
    "forbidden",
  );
  await page.mouse.click(center.x, center.y, { button: "right" });
  await expect.poll(() => commands.at(-1)?.[0]).toBe(25);
  expect(commands.at(-1)?.at(-1)).toBe(20);
});

test("villagers see build and repair cursors and send construction commands", async ({
  page,
}, info) => {
  test.setTimeout(90_000);
  const commands = await inputFixture(
    page,
    info.project.name === "webgpu" ? "webgpu" : "webgl2",
    [
      building(20, 1, 8193, 8188, 2, 100, false),
      building(21, 1, 8196, 8189, 2, 400),
      building(22, 1, 8198, 8191, 2),
      building(23, 1, 8187, 8190, 2, 400, true, 1),
    ],
    true,
  );
  await selectWorker(page);
  const canvas = page.locator("#scene");
  for (const [x, y, id] of [
    [8194, 8189, 20],
    [8197, 8190, 21],
  ]) {
    const p = screen(x ?? 0, y ?? 0);
    await page.mouse.move(p.x, p.y);
    await expect(canvas).toHaveAttribute("data-command-cursor", "build");
    await expect
      .poll(() => canvas.evaluate((el) => getComputedStyle(el).cursor))
      .toContain("data:image/png");
    const before = commands.length;
    await page.mouse.click(p.x, p.y, { button: "right" });
    await expect.poll(() => commands.length).toBeGreaterThan(before);
    expect(commands.at(-1)?.at(-1)).toBe(id);
  }
  const healthy = screen(8199, 8192);
  await page.mouse.move(healthy.x, healthy.y);
  await expect(canvas).not.toHaveAttribute("data-command-cursor", "build");
  const foreign = screen(8188, 8191);
  await page.mouse.move(foreign.x, foreign.y);
  await expect(canvas).toHaveAttribute("data-command-cursor", "attack");
});

test("huntable and hostile animals use attack artwork for villagers and military", async ({
  page,
}, info) => {
  test.setTimeout(90_000);
  const commands = await inputFixture(
    page,
    info.project.name === "webgpu" ? "webgpu" : "webgl2",
  );
  for (const worker of [true, false]) {
    if (worker) await selectWorker(page);
    else await page.mouse.click(640, 360);
    for (const [x, y] of [
      [8189.5, 8195.5],
      [8190.5, 8197.5],
      [8192.5, 8198.5],
    ]) {
      const p = screen(x ?? 0, y ?? 0);
      await page.mouse.move(p.x, p.y);
      await expect(page.locator("#scene")).toHaveAttribute(
        "data-command-cursor",
        worker ? "hunt" : "attack",
      );
      expect(
        await page
          .locator("#scene")
          .evaluate((el) => getComputedStyle(el).cursor),
      ).toContain("url(");
      const before = commands.length;
      await page.mouse.click(p.x, p.y, { button: "right" });
      await expect.poll(() => commands.length).toBeGreaterThan(before);
      expect(commands.at(-1)?.[0]).toBe(worker ? 15 : 7);
    }
  }
});
