import { expect, test, type Page } from "@playwright/test";
import { gameAssets } from "./game-assets.js";

async function cameraPosition(page: Page) {
  const text = await page.locator("#world-position").innerText();
  return text.split(",").map(Number) as [number, number];
}

test("camera keys follow screen directions and recenter finds the horse", async ({
  page,
  request,
}) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
  await page.goto("/");
  await expect(page.locator("#connection")).toHaveText("connected");
  await page.getByRole("button", { name: "Center on primary unit" }).click();
  await expect.poll(() => cameraPosition(page)).toEqual([8192.5, 8192.5]);
  const canvas = page.locator("#scene");
  const box = await canvas.boundingBox();
  if (!box) throw new Error("game canvas is missing");
  await page.mouse.move(box.width / 2, box.height / 2);
  const origin = await cameraPosition(page);
  const directions = [
    ["ArrowRight", 1, -1],
    ["ArrowLeft", -1, 1],
    ["ArrowDown", 1, 1],
    ["ArrowUp", -1, -1],
  ] as const;
  for (const [key, xSign, ySign] of directions) {
    await page.keyboard.press(key);
    await expect
      .poll(async () => {
        const point = await cameraPosition(page);
        return (
          (point[0] - origin[0]) * xSign > 0 &&
          (point[1] - origin[1]) * ySign > 0
        );
      })
      .toBe(true);
    await page.getByRole("button", { name: "Center on primary unit" }).click();
    await expect.poll(() => cameraPosition(page)).toEqual(origin);
    await page.mouse.move(box.width / 2, box.height / 2);
  }
  await page.mouse.wheel(0, 5_000);
  await page.keyboard.press("ArrowRight");
  await expect
    .poll(async () => (await cameraPosition(page))[0])
    .toBeGreaterThan(origin[0]);
  await page.getByRole("button", { name: "Center on primary unit" }).click();
  await expect.poll(() => cameraPosition(page)).toEqual(origin);
});
