import { expect } from "@playwright/test";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { fallbackTest as test, forceWebGl } from "./backends.js";

test("original-art cockpit proof", async ({ browser, baseURL }, info) => {
  test.skip(
    info.project.name !== "browser-defaults",
    "Normal WebGL compositor take",
  );
  const config = await readFile("../.cache/showcase/cockpit.json", "utf8").then(
    (text) => JSON.parse(text) as { stage: string; scenario?: string },
    () => null,
  );
  test.skip(!config, "Opt-in local original-art proof");
  if (!config) return;
  test.setTimeout(120_000);
  await mkdir("../.cache/showcase", { recursive: true });
  const context = await browser.newContext({
    baseURL,
    viewport: { width: 1280, height: 720 },
    recordVideo: {
      dir: "../.cache/showcase/raw",
      size: { width: 1280, height: 720 },
    },
  });
  const page = await context.newPage();
  const started = Date.now();
  await forceWebGl(page);
  try {
    expect(
      (
        await page.request.post(
          `/maps/reset?scenario=${config.scenario ?? "general-army"}`,
        )
      ).status(),
    ).toBe(204);
    await page.goto("/?play=1#x=106&y=128&zoom=.5&renderer=webgl2");
    await expect(page.locator("#connection")).toHaveText("connected", {
      timeout: 60_000,
    });
    await expect(page.locator("html")).toHaveAttribute(
      "data-interface-art",
      "original",
    );
    await expect(page.locator("#playground")).toHaveAttribute(
      "data-renderer",
      "webgl2",
    );
    if (!config.scenario || config.scenario === "general-army") {
      await expect
        .poll(async () => {
          const actors = JSON.parse(
            (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
          ) as { kind: string; x: number; y: number }[];
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
            (
              await page.locator("#scene").getAttribute("data-actors")
            )?.includes('"General"'),
          { timeout: 30_000 },
        )
        .toBe(true);
    }
    await page.waitForTimeout(1200);
    const ready = (Date.now() - started) / 1000;
    await page.screenshot({
      path: `../.cache/showcase/${config.stage}-near.png`,
    });
    await page.evaluate(() => {
      location.hash = "x=128&y=128&zoom=.04";
    });
    await expect(page.locator("#scene")).toHaveAttribute("data-lod", "markers");
    await page.waitForTimeout(1200);
    await page.screenshot({
      path: `../.cache/showcase/${config.stage}-far.png`,
    });
    if (config.stage.startsWith("item1")) {
      await expect(page.locator("#strategic-cockpit")).toBeVisible();
      await expect(
        page.locator("#strategic-cockpit .original-icon").first(),
      ).toBeVisible();
      await page.waitForTimeout(3000);
      await page.screenshot({
        path: `../.cache/showcase/${config.stage}-last.png`,
      });
    }
    await writeFile(
      `../.cache/showcase/${config.stage}.json`,
      JSON.stringify(
        {
          ready,
          duration: (Date.now() - started) / 1000 - ready,
          camera: await page
            .locator("#minimap")
            .evaluate((e) => ({ ...e.dataset })),
          cockpit: (await page.locator("#strategic-cockpit").count())
            ? await page.locator("#strategic-cockpit").textContent()
            : null,
          health: (await (await page.request.get("/health")).json()) as unknown,
        },
        null,
        2,
      ),
    );
  } finally {
    await context.close();
    await page.video()?.saveAs(`../.cache/showcase/${config.stage}.webm`);
  }
});
