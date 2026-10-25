import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { building, inputFixture, screen, selectWorker } from "./fixture.js";

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
