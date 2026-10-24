import { expect } from "@playwright/test";
import { readFile, writeFile } from "node:fs/promises";
import { test } from "../../server.js";
import { forceWebGl } from "../rendering/backends.js";
import { centerVillage } from "../village/fixtures/resources.js";

test("local original panel proof with real selection and training", async ({
  page,
}) => {
  const config = await readFile("../.cache/ui/proof.json", "utf8").then(
    (text) => JSON.parse(text) as { phase: string; url: string },
    () => null,
  );
  test.skip(!config, "Local panel proof was not requested");
  if (!config) return;
  test.setTimeout(180_000);
  expect(config.url).toBe("http://127.0.0.1:8141/");
  const health = await page.request.get(`${config.url}health`);
  expect(health.ok()).toBe(true);
  await page.setViewportSize({ width: 1280, height: 720 });
  await forceWebGl(page);
  expect((await page.request.post(`${config.url}maps/reset`)).status()).toBe(
    204,
  );
  await page.goto(`${config.url}?play=1`);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await centerVillage(page, true);
  const actors = async () =>
    JSON.parse(
      (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
    ) as { kind: string; x: number; y: number }[];
  const select = async (kind: string) => {
    const a = (await actors()).find((a) => a.kind === kind);
    if (!a) throw new Error(`${kind} missing`);
    await page.mouse.click(a.x, a.y);
    await expect(page.locator("#selection")).toHaveAttribute("data-kind", kind);
  };
  const capture = async (name: string) => {
    await page.screenshot({ path: `../.cache/ui/${config.phase}-${name}.png` });
    await writeFile(
      `../.cache/ui/${config.phase}-${name}.json`,
      JSON.stringify(
        {
          health: await health.json(),
          actors: await actors(),
          selection: await page.locator("#selection").innerText(),
          interfaceArt: await page
            .locator("html")
            .getAttribute("data-interface-art"),
        },
        null,
        2,
      ),
    );
  };
  await capture("town-center");
  await select("Villager");
  await capture("villager");
  const villager = (await actors()).find((a) => a.kind === "Villager");
  if (!villager) throw new Error("Villager missing");
  await page.mouse.dblclick(villager.x, villager.y);
  await expect(page.locator("#selected-icons .unit-token")).toHaveCount(3);
  await capture("multi-selection");
  await select("Villager");
  await page.locator("#build-military-menu").click();
  await page.locator("#build-barracks").click();
  let placed = false;
  for (const [x, y] of [
    [420, 320],
    [850, 320],
    [420, 420],
    [850, 420],
    [640, 220],
  ]) {
    if (x === undefined || y === undefined) continue;
    await page.mouse.move(x, y);
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-order-mode",
      "build",
    );
    await page.waitForTimeout(350);
    if (
      (await page.locator("#scene").getAttribute("data-placement-valid")) ===
      "true"
    ) {
      await page.mouse.click(x, y);
      placed = true;
      break;
    }
  }
  expect(placed).toBe(true);
  await expect
    .poll(async () => (await actors()).some((a) => a.kind === "Barracks"))
    .toBe(true);
  await select("Barracks");
  await expect(page.locator("#building-status")).toHaveText("Complete", {
    timeout: 85_000,
  });
  await page.locator("#train-villager").click();
  await expect(page.locator("#training-queue .unit-token")).toHaveCount(1);
  await capture("barracks-queue");
});
