import { readFile } from "node:fs/promises";
import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { centerVillage, freshHud } from "../fixtures/resources.js";

test("a production building sets a rally flag and its trained villager walks there after reload", async ({
  page,
  request,
}) => {
  test.setTimeout(150_000);
  const capture = await readFile("../.cache/qol/capture.json", "utf8")
    .then((text) => JSON.parse(text) as { url: string })
    .catch(() => null);
  if (capture && capture.url !== "http://127.0.0.1:8131/")
    throw new Error("Unexpected quality-of-life capture origin");
  const base = capture?.url ?? "/";
  expect((await request.post(`${base}maps/reset`)).status()).toBe(204);
  await gameAssets(page);
  await page.goto(`${base}?play=1#zoom=.5`);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await centerVillage(page, true);
  const canvas = page.locator("#scene");
  const before = JSON.parse(
    (await canvas.getAttribute("data-actors")) ?? "[]",
  ) as { id: number; kind: string; x: number; y: number }[];
  const center = before.find((a) => a.kind === "TownCenter");
  if (!center) throw new Error("Town Center missing");
  // The clear southern spawn apron is outside the Town Center footprint.
  const target = { x: center.x, y: center.y + 110 };
  await page.mouse.click(target.x, target.y, { button: "right" });
  await expect(canvas).toHaveAttribute("data-rally-point", /.+/);
  const rally = await canvas.getAttribute("data-rally-point");
  await page.reload();
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await centerVillage(page, true);
  await expect(canvas).toHaveAttribute("data-rally-point", rally ?? "");
  await page.locator("#train-villager").click();
  await expect
    .poll(
      async () => {
        await freshHud(page);
        const actors = JSON.parse(
          (await canvas.getAttribute("data-actors")) ?? "[]",
        ) as { id: number; kind: string; x: number; y: number }[];
        const trained = actors.find(
          (a) => a.kind === "Villager" && !before.some((b) => b.id === a.id),
        );
        return trained
          ? Math.hypot(trained.x - target.x, trained.y - target.y)
          : Infinity;
      },
      { timeout: 70_000 },
    )
    .toBeLessThan(12);
  if (capture) {
    await page.screenshot({
      path: `../.cache/qol/rally-${test.info().project.name}.png`,
    });
  }
});
