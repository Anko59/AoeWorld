import { expect, test } from "@playwright/test";
import { readFile, writeFile } from "node:fs/promises";
import { centerVillage, frameResource } from "./resources.js";

// An opt-in local recording aid. Ordinary browser gates never need a dev server.
test("prepare a live berry worker for the narrated village recording", async ({
  page,
}) => {
  const config = await readFile(
    "../.cache/interface-video-prepare.json",
    "utf8",
  )
    .then((body) => JSON.parse(body) as { url: string })
    .catch(() => null);
  test.skip(!config, "Local video preparation was not requested");
  if (!config || config.url !== "http://127.0.0.1:18083/")
    throw new Error(
      "Recording preparation must use this task's local dev server",
    );
  test.setTimeout(180_000);
  await page.goto(`${config.url}#zoom=.35`);
  await expect(page.locator("#population")).toHaveText("4/5", {
    timeout: 30_000,
  });
  await centerVillage(page);
  await page.locator("#scene").focus();
  const actor = await page.locator("#scene").evaluate((canvas) =>
    (
      JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as {
        kind: string;
        x: number;
        y: number;
      }[]
    ).find((a) => a.kind === "Villager"),
  );
  if (!actor) throw new Error("Village worker missing");
  await page.mouse.click(actor.x, actor.y);
  await expect(page.locator("#selection-name")).toHaveText("Villager");
  const berries = await frameResource(page, 0, 0.25);
  await page.mouse.click(berries.x, berries.y, { button: "right" });
  await expect(page.locator("#selection-task")).toContainText("Gathering", {
    timeout: 85_000,
  });
  // Leave collection and the return trip running during the before scene.
  await writeFile(
    "../.cache/interface-shots/berry-preparation.json",
    JSON.stringify(
      {
        prepared_at: new Date().toISOString(),
        task: await page.locator("#selection-task").textContent(),
        food: await page.locator("#stockpile-food").textContent(),
      },
      null,
      2,
    ),
  );
});
