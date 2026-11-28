import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { forceCanvas, forceWebGl } from "./backends.js";

test("a General is selected and marched without leaving far zoom", async ({
  page,
  request,
}, info) => {
  test.setTimeout(90_000);
  expect(
    (await request.post("/maps/reset?scenario=general-army")).status(),
  ).toBe(204);
  if (info.project.name === "canvas") await forceCanvas(page);
  else if (info.project.name !== "webgpu") await forceWebGl(page);
  await gameAssets(page);
  await page.goto("/?play=1#x=106&y=128&zoom=.5");
  await expect(page.locator("#connection")).toHaveText("connected");
  type Actor = { id: number; kind: string; x: number; y: number };
  await expect
    .poll(async () => {
      const actors: Actor[] = JSON.parse(
        (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
      );
      const castle = actors.find((a) => a.kind === "Castle");
      if (!castle) return false;
      await page.mouse.click(castle.x, castle.y);
      return page.locator("#train-general").isVisible();
    })
    .toBe(true);
  await page.locator("#train-general").click();
  await expect
    .poll(
      async () =>
        (await page.locator("#scene").getAttribute("data-actors"))?.includes(
          '"General"',
        ),
      { timeout: 30_000 },
    )
    .toBe(true);
  await page.evaluate(() => {
    location.hash = "x=112&y=128&zoom=.04";
  });
  await expect(page.locator("#scene")).toHaveAttribute("data-lod", "markers");
  const perfEnabled = () =>
    page.evaluate(
      () =>
        (window as Window & { aoePerformance?: { enabled: boolean } })
          .aoePerformance?.enabled,
    );
  expect(await perfEnabled()).toBe(false);
  await page.keyboard.press("F3");
  await expect(
    page.getByRole("button", { name: /Economy F3/ }),
  ).toHaveAttribute("aria-pressed", "false");
  expect(await perfEnabled()).toBe(false);
  await page.keyboard.press("F3");
  await page.keyboard.press("F8");
  expect(await perfEnabled()).toBe(true);
  await page.keyboard.press("F8");
  expect(await perfEnabled()).toBe(false);
  const token = page.locator(".army-token[data-owned='true']").first();
  await expect(token).toContainText("14");
  await token.click();
  await expect(page.locator("#selection")).toHaveAttribute(
    "data-kind",
    "General",
  );
  await expect(page.locator("#general-panel")).toBeVisible();
  await page.locator("#general-only-move").hover();
  await page.mouse.click(810, 380, { button: "right" });
  await expect(page.locator("#general-panel")).toHaveAttribute(
    "data-state",
    "marching",
  );
  await expect(page.locator("#scene")).toHaveAttribute("data-lod", "markers");
  await expect(token).toHaveAttribute("data-order", "marching");
  await page.evaluate(() => {
    location.hash = "x=112&y=128&zoom=.5";
  });
  await expect(page.locator("#scene")).toHaveAttribute("data-lod", "sprites");
  await expect(token).toBeHidden();
  await page.keyboard.press("F3");
  expect(await perfEnabled()).toBe(true);
  await page.keyboard.press("F3");
});
