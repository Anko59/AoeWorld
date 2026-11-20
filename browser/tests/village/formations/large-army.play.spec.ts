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

test("300 soldiers draw one line, breach an enemy wall and clear orders on Stop", async ({
  page,
  request,
}, info) => {
  test.setTimeout(110_000);
  expect(
    (await request.post("/maps/reset?scenario=army-orders")).status(),
  ).toBe(204);
  await gameAssets(page);
  const renderer = info.project.name === "firefox-webgl2" ? "webgl2" : "webgpu";
  await page.goto(`/?play=1#x=113&y=127&zoom=.38&renderer=${renderer}`);
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
  // These ground points stay above the command panel at this viewport.
  await page.mouse.click(wall.x - 150, wall.y - 35, { button: "right" });
  const lines = page.locator("#waypoint-markers > path[d]:not([d=''])");
  const markers = page.locator("#waypoint-markers > g");
  await expect(markers).toHaveCount(1);
  const moveLines = await lines.count();
  await page.waitForTimeout(900);
  await page.locator("#attack-move-units").click();
  await page.mouse.click(wall.x, wall.y, { button: "right" });
  // Record a fixed window on both builds so the baseline captures the refusal.
  await page.waitForTimeout(16_000);
  const breached = (await actors()).every(
    (a) => a.id !== wall.id || a.hp === 0,
  );
  await page.mouse.click(wall.x + 95, wall.y - 32, { button: "right" });
  await expect(markers).toHaveCount(1);
  await page.locator("#stop-units").click();
  await expect(markers).toHaveCount(0);
  await expect(lines).toHaveCount(0);
  await page.waitForTimeout(900);
  expect(moveLines).toBe(1);
  expect(breached).toBe(true);
});
