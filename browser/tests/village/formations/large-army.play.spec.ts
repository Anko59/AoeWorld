import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";

type Actor = {
  id: number;
  kind: string;
  x: number;
  y: number;
  hp: number;
  selected: boolean;
};

test.use({ video: "on", viewport: { width: 1280, height: 720 } });

test("300 soldiers attack-move through a wall on the way to open ground", async ({
  page,
  request,
}, info) => {
  test.setTimeout(110_000);
  expect(
    (await request.post("/maps/reset?scenario=army-orders")).status(),
  ).toBe(204);
  await gameAssets(page);
  const renderer = info.project.name === "firefox-webgl2" ? "webgl2" : "webgpu";
  await page.goto(`/?play=1#x=115&y=127&zoom=.38&renderer=${renderer}`);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  const scene = page.locator("#scene");
  const actors = () =>
    scene.evaluate(
      (node) => JSON.parse(node.getAttribute("data-actors") ?? "[]") as Actor[],
    );
  await expect
    .poll(
      async () => (await actors()).filter((a) => a.kind === "Militia").length,
    )
    .toBe(300);
  const units = (await actors()).filter((a) => a.kind === "Militia");
  const wall = (await actors()).find((a) => a.kind === "PalisadeWall");
  const first = units[Math.floor(units.length / 2)];
  if (!first || !wall) throw new Error("Army showpiece is outside the camera");
  await page.mouse.dblclick(first.x, first.y);
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "300");
  const center = {
    x: units.reduce((sum, unit) => sum + unit.x, 0) / units.length,
    y: units.reduce((sum, unit) => sum + unit.y, 0) / units.length,
  };
  const dx = wall.x - center.x;
  const dy = wall.y - center.y;
  // Aim beyond the wall on the line from the army to the wall.
  const destination = { x: wall.x + dx * 0.25, y: wall.y + dy * 0.25 };
  await page.locator("#attack-move-units").click();
  await expect(scene).toHaveAttribute("data-order-mode", "attack-move");
  expect(
    await scene.evaluate(
      (canvas, { x, y }) => document.elementFromPoint(x, y) === canvas,
      destination,
    ),
  ).toBe(true);
  await page.mouse.click(destination.x, destination.y, { button: "right" });
  const lines = page.locator("#waypoint-markers > path[d]:not([d=''])");
  const markers = page.locator("#waypoint-markers > g");
  await expect(markers).toHaveCount(1);
  await expect(lines).toHaveCount(1);
  await expect
    .poll(async () => (await actors()).find((a) => a.id === wall.id)?.hp ?? 0, {
      timeout: 24_000,
    })
    .toBe(0);
  await expect
    .poll(
      async () =>
        (await actors()).filter(
          (a) =>
            a.kind === "Militia" &&
            (a.x - wall.x) * dx + (a.y - wall.y) * dy > 0,
        ).length,
    )
    .toBeGreaterThan(0);
  // Keep the breached line on screen before the Stop check.
  await page.waitForTimeout(900);
  await page.mouse.click(wall.x + 95, wall.y - 32, { button: "right" });
  await expect(markers).toHaveCount(1);
  await page.locator("#stop-units").click();
  await expect(markers).toHaveCount(0);
  await expect(lines).toHaveCount(0);
  await page.waitForTimeout(900);
});
