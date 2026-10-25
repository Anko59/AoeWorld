import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { readFile, writeFile } from "node:fs/promises";
import {
  centerVillage,
  frameResource,
  freshHud,
  resourceTargets,
} from "./fixtures/resources.js";

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
  const candidates = JSON.parse(
    await readFile("../.cache/interface-capture-map-list.json", "utf8"),
  ) as { hash: string; seed: number }[];
  let chosen = false;
  for (const candidate of candidates) {
    const token = await page.evaluate(() =>
      localStorage.getItem("aoeworld.resume-token"),
    );
    if (!token) throw new Error("Recording controller token missing");
    expect(
      (
        await page.request.post(`${config.url}maps/${candidate.hash}`, {
          headers: { "X-AoeWorld-Controller-Token": token },
        })
      ).ok(),
    ).toBeTruthy();
    await page.getByRole("button", { name: "Menu", exact: true }).click();
    await page.getByRole("button", { name: "Reconnect to server" }).click();
    await expect(page.locator("#minimap")).toHaveAttribute(
      "data-map-hash",
      candidate.hash,
    );
    await centerVillage(page);
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-chunks-pending",
      "0",
      { timeout: 20_000 },
    );
    const worker = await page.locator("#scene").evaluate((canvas) =>
      (
        JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as {
          id: number;
          x: number;
          y: number;
        }[]
      ).find((actor) => actor.id === 2),
    );
    if (!worker) throw new Error("Recording berry worker missing");
    await page.mouse.click(worker.x, worker.y);
    await freshHud(page);
    const food = (await resourceTargets(page)).find((node) => node.kind === 0);
    if (!food || food.distance > 900) continue;
    await writeFile(
      "../.cache/interface-shots/video-map.json",
      JSON.stringify({ ...candidate, food }, null, 2),
    );
    chosen = true;
    break;
  }
  if (!chosen)
    throw new Error("No short visible berry route in the capture maps");
  await page.setViewportSize({ width: 1280, height: 600 });
  await centerVillage(page);
  const woodWorker = await page.locator("#scene").evaluate((canvas) =>
    (
      JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as {
        id: number;
        x: number;
        y: number;
      }[]
    ).find((actor) => actor.id === 3),
  );
  if (!woodWorker) throw new Error("Recording wood worker missing");
  await page.mouse.click(woodWorker.x, woodWorker.y);
  await frameResource(page, 1);
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-chunks-pending",
    "0",
    { timeout: 20_000 },
  );
  await freshHud(page);
  const points = await page.locator("#scene").evaluate((canvas) => ({
    camera: {
      x: Number(
        document.getElementById("minimap")?.getAttribute("data-camera-x"),
      ),
      y: Number(
        document.getElementById("minimap")?.getAttribute("data-camera-y"),
      ),
      zoom: Number(
        document.getElementById("minimap")?.getAttribute("data-zoom"),
      ),
    },
    actors: JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as unknown,
    resources: JSON.parse(
      canvas.getAttribute("data-resource-targets") ?? "[]",
    ) as unknown,
  }));
  await writeFile(
    "../.cache/interface-shots/video-points.json",
    JSON.stringify(points, null, 2),
  );
  await page.setViewportSize({ width: 1280, height: 720 });
  await centerVillage(page);
  await page.locator("#scene").focus();
  const actor = await page.locator("#scene").evaluate((canvas) =>
    (
      JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as {
        id: number;
        kind: string;
        x: number;
        y: number;
      }[]
    ).find((a) => a.id === 2),
  );
  if (!actor) throw new Error("Village worker missing");
  await page.mouse.click(actor.x, actor.y);
  await expect(page.locator("#selection-name")).toHaveText("Villager");
  const berries = await frameResource(page, 0, 0.25);
  await page.mouse.click(berries.x, berries.y, { button: "right" });
  // Keep the recording setup close to the berries while the worker travels.
  await page.mouse.wheel(0, -900);
  await page.keyboard.press("m");
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
