import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { building, inputFixture, screen, selectWorker } from "./fixture.js";

// Frozen authoritative state exercises real picking, Dock availability and
// encoded commands. Simulation tests cover the resulting load/unload and combat.
test("river Dock offers Feudal ships and Castle research requirements", async ({
  page,
}) => {
  const commands = await inputFixture(
    page,
    "webgl2",
    [building(10, 9, 8188, 8190, 3)],
    false,
    0,
    undefined,
    { cargo: 0 },
  );
  const dock = screen(8189.5, 8191.5);
  await page.mouse.click(dock.x, dock.y);
  await expect(page.locator("#selection-name")).toHaveText("Dock");
  for (const kind of [
    "galley",
    "fire-ship",
    "demolition-ship",
    "transport-ship",
  ]) {
    await expect(page.locator(`#train-${kind}`)).toBeEnabled();
  }
  await expect(page.locator("#research-war-galley")).toBeDisabled();
  await expect(page.locator("#research-war-galley")).toHaveAttribute(
    "title",
    /requires Castle Age/,
  );
  await page.locator("#train-galley").click();
  await expect
    .poll(() => commands.find((bytes) => bytes[0] === 5)?.slice(-2))
    .toEqual([10, 17]);
});

test("land units right-click a Transport Ship to send a bounded load order", async ({
  page,
}) => {
  const commands = await inputFixture(page, "webgl2", [], false, 0, undefined, {
    cargo: 0,
  });
  await selectWorker(page);
  const ship = screen(8193.5, 8194.5);
  await page.mouse.click(ship.x, ship.y, { button: "right" });
  await expect
    .poll(() => commands.find((bytes) => bytes[0] === 27)?.slice(2))
    .toEqual([1, 2, 5]);
});

test("Transport Ship displays passengers and unloads onto the chosen bank", async ({
  page,
}) => {
  const commands = await inputFixture(page, "webgl2", [], false, 0, undefined, {
    cargo: 2,
  });
  const ship = screen(8193.5, 8194.5);
  await page.mouse.click(ship.x, ship.y);
  await expect(page.locator("#selection-name")).toHaveText("Transport Ship");
  await expect(page.locator("#selection-load")).toHaveText("2/5 passengers");
  await expect(page.locator("#unload-transport")).toBeEnabled();
  await page.locator("#unload-transport").click();
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-order-mode",
    "unload",
  );
  const bank = screen(8194.5, 8194.5);
  await page.mouse.click(bank.x, bank.y, { button: "right" });
  await expect
    .poll(() => commands.find((bytes) => bytes[0] === 28)?.[2])
    .toBe(5);
});

test("explicit and queued Move keep their meaning beside a transport", async ({
  page,
}) => {
  const commands = await inputFixture(page, "webgl2", [], false, 0, undefined, {
    cargo: 2,
  });
  const ship = screen(8193.5, 8194.5);
  await page.mouse.click(ship.x, ship.y);
  await page.locator("#unload-transport").click();
  await page.locator("#move-units").click();
  const bank = screen(8194.5, 8194.5);
  await page.mouse.click(bank.x, bank.y, { button: "right" });
  await expect.poll(() => commands.some((bytes) => bytes[0] === 2)).toBe(true);
  expect(commands.some((bytes) => bytes[0] === 28)).toBe(false);
  await selectWorker(page);
  await page.keyboard.down("Shift");
  await page.mouse.click(ship.x, ship.y, { button: "right" });
  await page.keyboard.up("Shift");
  await expect.poll(() => commands.some((bytes) => bytes[0] === 21)).toBe(true);
  expect(commands.some((bytes) => bytes[0] === 27)).toBe(false);
});

test("selecting a worker after arming unload preserves its ordinary move order", async ({
  page,
}) => {
  const commands = await inputFixture(page, "webgl2", [], false, 0, undefined, {
    cargo: 2,
  });
  const ship = screen(8193.5, 8194.5);
  await page.mouse.click(ship.x, ship.y);
  await page.locator("#unload-transport").click();
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-order-mode",
    "unload",
  );
  await selectWorker(page);
  const bank = screen(8194.5, 8194.5);
  await page.mouse.click(bank.x, bank.y, { button: "right" });
  await expect
    .poll(() => commands.find((bytes) => bytes[0] === 2)?.[2])
    .toBe(2);
  expect(commands.some((bytes) => bytes[0] === 28)).toBe(false);
});
