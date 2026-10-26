import { expect } from "@playwright/test";
import { mkdir, readFile } from "node:fs/promises";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { centerVillage, freshHud } from "../fixtures/resources.js";

test("villagers garrison through the cursor and eject individually or together", async ({
  page,
  request,
}) => {
  test.setTimeout(150_000);
  // Optional screenshots use only this checkout's own `make dev DEV_PORT=8147`.
  // Only Firefox shares the screenshot server; other projects keep their
  // isolated worker server so concurrent match resets cannot interfere.
  const capture =
    test.info().project.name === "firefox-webgl2"
      ? await readFile("../.cache/garrison/capture.json", "utf8")
          .then((text) => JSON.parse(text) as { url: string })
          .catch(() => null)
      : null;
  if (capture && capture.url !== "http://127.0.0.1:8147/")
    throw new Error("Unexpected garrison capture origin");
  const base = capture?.url ?? "/";
  expect((await request.post(`${base}maps/reset`)).status()).toBe(204);
  if (!capture || !(await request.get(`${base}asset-pack/manifest.json`)).ok())
    await gameAssets(page, true);
  await page.goto(`${base}?play=1`);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await centerVillage(page);
  const actors = async () =>
    JSON.parse(
      (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
    ) as { id: number; kind: string; x: number; y: number }[];
  const villagers = (await actors()).filter(
    (actor) => actor.kind === "Villager",
  );
  const center = (await actors()).find((actor) => actor.kind === "TownCenter");
  if (villagers.length < 2 || !center)
    throw new Error("Starting village missing");
  const population = await page.locator("#population").textContent();
  for (const worker of villagers.slice(0, 2)) {
    await freshHud(page);
    const current = (await actors()).find((actor) => actor.id === worker.id);
    if (!current) throw new Error("Villager vanished before entry");
    await page.mouse.click(current.x, current.y);
    await expect(page.locator("#selection-name")).toContainText("Villager");
    await page.locator("#garrison").click();
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-order-mode",
      "garrison",
    );
    if (worker.id === villagers[0]?.id) {
      await page.keyboard.press("Escape");
      await expect(page.locator("#scene")).toHaveAttribute(
        "data-order-mode",
        "",
      );
      await expect(page.locator("#selection-name")).toContainText("Villager");
    }
    await page.mouse.move(center.x, center.y);
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-command-cursor",
      "garrison",
    );
    await expect(page.locator("#scene")).not.toHaveCSS("cursor", "default");
    await page.mouse.click(center.x, center.y, {
      button: worker.id === villagers[0]?.id ? "right" : "left",
    });
    await expect
      .poll(
        async () => (await actors()).some((actor) => actor.id === worker.id),
        { timeout: 20_000 },
      )
      .toBe(false);
  }
  await page.mouse.click(center.x, center.y);
  await expect(page.locator("#selection-name")).toHaveText("Town Center");
  await expect(page.locator("#garrison-count")).toHaveText("2 / 15");
  await expect(page.locator("#garrison-units button")).toHaveCount(2);
  for (const worker of villagers.slice(0, 2)) {
    const button = page.locator(
      `#garrison-units button[data-unit-id="${worker.id}"]`,
    );
    await expect(button).toHaveAccessibleName(/Ungarrison Villager.*HP/);
    await expect(
      button.locator('[data-ui-group="units"][data-ui-id="83"] svg'),
    ).toBeVisible();
  }
  if (capture) {
    await mkdir("../.cache/garrison", { recursive: true });
    await page.screenshot({
      path: `../.cache/garrison/garrison-${test.info().project.name}.png`,
    });
  }
  await expect(page.locator("#population")).toHaveText(population ?? "");
  await page.locator("#garrison-units button").first().click();
  await expect(page.locator("#garrison-count")).toHaveText("1 / 15");
  await expect
    .poll(
      async () =>
        (await actors()).filter((actor) => actor.kind === "Villager").length,
    )
    .toBe(villagers.length - 1);
  await page.locator("#ungarrison").click();
  await expect(page.locator("#garrison-count")).toHaveText("0 / 15");
  await expect
    .poll(
      async () =>
        (await actors()).filter((actor) => actor.kind === "Villager").length,
    )
    .toBe(villagers.length);
  await expect(page.locator("#ungarrison")).toBeDisabled();
  await expect(page.locator("#population")).toHaveText(population ?? "");
  if (capture)
    await page.screenshot({
      path: `../.cache/garrison/ejected-${test.info().project.name}.png`,
    });
});
