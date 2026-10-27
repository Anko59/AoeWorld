import { expect, type Page } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";

async function position(page: Page): Promise<[number, number]> {
  return (await page.locator("#world-position").innerText())
    .split(",")
    .map(Number) as [number, number];
}

test("whole-map navigation jumps locally, returns home, and held keys release", async ({
  page,
  request,
}) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected");
  await page.getByRole("button", { name: "Center on primary unit" }).click();
  await expect.poll(() => position(page)).toEqual([8192.5, 8192.5]);
  const start = await position(page);
  await page.mouse.move(600, 300);
  await page.keyboard.down("ArrowRight");
  await expect
    .poll(async () => (await position(page))[0] - start[0])
    .toBeGreaterThan(2);
  await page.keyboard.up("ArrowRight");
  await expect
    .poll(() => page.locator("#world-position").textContent())
    .toBeTruthy();
  const stopped = await position(page);
  await page.waitForTimeout(150);
  await expect.poll(() => position(page)).toEqual(stopped);
  await page.getByRole("button", { name: "Show whole map" }).click();
  await expect(page.locator("#minimap")).toHaveClass(/expanded/);
  const map = page.locator("#minimap-map");
  const bounds = await map.boundingBox();
  if (!bounds) throw new Error("Minimap missing");
  const side = Math.min(bounds.width, bounds.height);
  await page.mouse.click(
    bounds.x + (bounds.width - side) / 2 + side * 0.75,
    bounds.y + (bounds.height - side) / 2 + side * 0.5,
  );
  await expect
    .poll(async () => {
      const [x, y] = await position(page);
      return Math.abs(x - 12_288) < 40 && Math.abs(y - 4096) < 40;
    })
    .toBe(true);
  const left = bounds.x + (bounds.width - side) / 2;
  const top = bounds.y + (bounds.height - side) / 2;
  await page.mouse.move(left + side * 0.5, top + side * 0.5);
  const beforeDrag = await position(page);
  const transform = await page
    .locator("#minimap-view")
    .getAttribute("transform");
  await page.mouse.down();
  await page.mouse.move(left + side * 0.6, top + side * 0.45, { steps: 4 });
  await page.mouse.up();
  await expect(page.locator("#minimap-view")).not.toHaveAttribute(
    "transform",
    transform ?? "",
  );
  expect(await position(page)).toEqual(beforeDrag);
  await page.getByRole("button", { name: "Close map overview" }).click();
  await expect(page.locator("#minimap")).not.toHaveClass(/expanded/);
  await page.keyboard.press("Home");
  await expect.poll(() => position(page)).toEqual(start);
  await page.evaluate(() => {
    const input = document.createElement("input");
    document.body.append(input);
    input.focus();
  });
  const typingStart = await position(page);
  await page.keyboard.press("ArrowRight");
  await page.waitForTimeout(150);
  await expect.poll(() => position(page)).toEqual(typingStart);
});
