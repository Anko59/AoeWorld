import { expect, type Page } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { mkdir, writeFile } from "node:fs/promises";
import { fiveVillagerFixture } from "./fixtures/village.js";
import {
  centerVillage,
  frameResource,
  freshHud,
} from "./fixtures/resources.js";

type Actor = { id: number; kind: string; x: number; y: number };
async function clickActor(page: Page, kind: string, id?: number) {
  const actor = await page
    .locator("#scene")
    .evaluate(
      (canvas, { kind, id }) =>
        (
          JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as Actor[]
        ).find((a) => a.kind === kind && (id === undefined || a.id === id)),
      { kind, id },
    );
  if (!actor) throw new Error(`Missing ${kind}`);
  await page.mouse.click(actor.x, actor.y);
}
async function shots(page: Page, name: string) {
  for (const [width, height] of [
    [1440, 900],
    [1280, 720],
  ] as const) {
    await page.setViewportSize({ width, height });
    await freshHud(page);
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-frame",
      /^[1-9]\d*$/,
    );
    await page.screenshot({
      path: `../.cache/interface-shots/${name}-${width}x${height}.png`,
    });
  }
}

test("private native-art interface capture: start, wood, training and five-villager fixture", async ({
  page,
}) => {
  test.setTimeout(120_000);
  const assets = await page.request.get("/asset-pack/manifest.json");
  test.skip(
    !assets.ok(),
    "Original art is private; ordinary CI exercises fixture markers.",
  );
  await mkdir("../.cache/interface-shots", { recursive: true });
  await gameAssets(page);
  expect((await page.request.post("/maps/reset")).status()).toBe(204);
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
  const maps = (await (await page.request.get("/maps")).json()) as {
    content_hash: string;
  }[];
  const hash = process.env["AOE_E2E_LANDSCAPE_HASH"] ?? maps[0]?.content_hash;
  if (!hash) throw new Error("Native capture requires a prepared resource map");
  await writeFile(
    "../.cache/interface-shots/village-map.json",
    JSON.stringify(await (await page.request.get(`/maps/${hash}`)).json()),
  );
  const token = await page.evaluate(() =>
    localStorage.getItem("aoeworld.resume-token"),
  );
  if (!token) throw new Error("Controller token missing");
  expect(
    (
      await page.request.post(`/maps/${hash}`, {
        headers: { "X-AoeWorld-Controller-Token": token },
      })
    ).ok(),
  ).toBeTruthy();
  await page.getByRole("button", { name: "Menu", exact: true }).click();
  await page.getByRole("button", { name: "Reconnect to server" }).click();
  await expect(page.locator("#minimap")).toHaveAttribute("data-map-hash", hash);
  await expect(page.locator("#stockpile-wood")).toHaveText("200");
  await centerVillage(page);
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-village-art",
    "native",
  );
  await page.locator("#scene").focus();
  await shots(page, "start");
  await clickActor(page, "Villager", 3);
  const wood = await frameResource(page, 1);
  await page.setViewportSize({ width: 1280, height: 600 });
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
    "../.cache/interface-shots/session-points.json",
    JSON.stringify(points, null, 2),
  );
  await page.setViewportSize({ width: 1280, height: 720 });
  await freshHud(page);
  await clickActor(page, "Villager", 3);
  await page.mouse.click(wood.x, wood.y, { button: "right" });
  await expect(page.locator("#selection-task")).toContainText("Gathering", {
    timeout: 35_000,
  });
  await expect
    .poll(async () =>
      page.locator("#scene").evaluate(
        (canvas) =>
          (
            JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as {
              selected: boolean;
              load?: [string, number];
            }[]
          ).find((actor) => actor.selected)?.load?.[1] ?? 0,
      ),
    )
    .toBeGreaterThan(0);
  await writeFile(
    "../.cache/interface-shots/gather-points.json",
    JSON.stringify(
      await page.locator("#scene").evaluate((canvas) => ({
        actors: JSON.parse(
          canvas.getAttribute("data-actors") ?? "[]",
        ) as unknown,
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
      })),
      null,
      2,
    ),
  );
  await page.locator("#scene").evaluate((canvas) => {
    const actor = (
      JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as {
        selected: boolean;
        x: number;
        y: number;
      }[]
    ).find((a) => a.selected);
    const minimap = document.getElementById("minimap");
    if (!actor || !minimap) throw new Error("Selected gatherer missing");
    const zoom = Number(minimap.getAttribute("data-zoom"));
    const dx = (actor.x - innerWidth / 2) / (64 * zoom),
      dy = (actor.y - innerHeight / 2) / (32 * zoom);
    const x = Number(minimap.getAttribute("data-camera-x")) + (dx + dy) / 2;
    const y = Number(minimap.getAttribute("data-camera-y")) + (dy - dx) / 2;
    location.hash = `x=${x}&y=${y}&zoom=.85`;
  });
  await shots(page, "gathering-wood");
  await centerVillage(page, true);
  await expect(page.locator("#selection-name")).toHaveText("Town Center");
  await page.keyboard.press("q");
  await expect(page.locator("#training-progress")).toBeVisible();
  await page.waitForTimeout(3500);
  await shots(page, "training-villager");
  // This final capture is explicitly a presentation fixture, with native art.
  expect((await page.request.post("/maps/reset")).status()).toBe(204);
  await fiveVillagerFixture(page);
  await page.goto("/?selection-fixture=1#x=8192.5&y=8192.5");
  await expect(page.locator("#population")).toHaveText("6", {
    timeout: 30_000,
  });
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-village-art",
    "native",
  );
  const villager = await page
    .locator("#scene")
    .evaluate((canvas) =>
      (JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as Actor[]).find(
        (a) => a.kind === "Villager",
      ),
    );
  if (!villager) throw new Error("Fixture villager missing");
  await page.mouse.dblclick(villager.x, villager.y);
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "5");
  await shots(page, "five-villagers-fixture");
});
