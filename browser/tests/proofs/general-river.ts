import { expect, type Page } from "@playwright/test";
import { writeFile } from "node:fs/promises";

export type RiverRecording = {
  contentHash: string;
  camera: [number, number];
  goal: [number, number];
  side: number;
  follow?: [number, number][];
};

/** The loaded source map and server-owned army are prepared before filming. */
export async function generalRiverProof(
  page: Page,
  url: string,
  setup: RiverRecording,
) {
  const map = await (
    await page.request.get(`${url}maps/${setup.contentHash}`)
  ).json();
  expect(map.source_locks.length).toBeGreaterThan(0);
  expect(map.environment.samples_per_axis).toBeGreaterThan(0);
  const started = Date.now();
  await page.goto(
    `${url}?play=1#x=${setup.camera[0]}&y=${setup.camera[1]}&zoom=0.4&renderer=webgl2`,
  );
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-village-art",
    "native",
  );
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-map-hash",
    setup.contentHash,
  );
  const actors = async () =>
    JSON.parse(
      (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
    ) as {
      id: number;
      kind: string;
      x: number;
      y: number;
      task: string;
    }[];
  await expect
    .poll(async () => (await actors()).some((a) => a.kind === "General"))
    .toBe(true);
  const general = (await actors()).find((a) => a.kind === "General");
  if (!general) throw new Error("General missing after scene initialization");
  expect(general.x).toBeGreaterThan(0);
  expect(general.x).toBeLessThan(1280);
  expect(general.y).toBeGreaterThan(64);
  expect(general.y).toBeLessThan(500);
  await page.mouse.click(general.x, general.y - 10);
  await expect(page.locator("#selection")).toHaveAttribute(
    "data-kind",
    "General",
  );
  await expect(page.locator("#vassal-status")).toContainText("100 soldiers");
  const snapshots: unknown[] = [];
  const capture = async (name: string) => {
    await page.screenshot({ path: `../.cache/ui/general-river-${name}.png` });
    snapshots.push({
      name,
      elapsedMs: Date.now() - started,
      actors: await actors(),
      camera: await page
        .locator("#minimap")
        .evaluate((e) => ({ ...(e as HTMLElement).dataset })),
    });
    await writeFile(
      "../.cache/ui/general-river-observations.json",
      JSON.stringify(
        { setup, sourceLocks: map.source_locks.length, started, snapshots },
        null,
        2,
      ),
    );
  };
  await capture("before");
  const mapClick = async (point: [number, number], button: "left") => {
    const panel = await page
      .locator("#minimap")
      .evaluate((e) => ({ ...(e as HTMLElement).dataset }));
    expect(Number(panel.minimapZoom)).toBe(1);
    const box = await page.locator("#minimap-map").boundingBox();
    if (!box) throw new Error("Minimap missing");
    const scale =
      (Math.min(box.width, box.height) * Number(panel.minimapZoom)) /
      (setup.side * 2);
    const dx = point[0] - setup.side / 2,
      dy = point[1] - setup.side / 2;
    await page.locator("#minimap-map").click({
      button,
      position: {
        x: box.width / 2 + (dx - dy) * scale,
        y: box.height / 2 + (dx + dy) * scale,
      },
    });
    await expect
      .poll(async () => {
        const camera = await page
          .locator("#minimap")
          .evaluate((e) => ({ ...(e as HTMLElement).dataset }));
        return (
          Math.abs(Number(camera.cameraX) - point[0]) +
          Math.abs(Number(camera.cameraY) - point[1])
        );
      })
      .toBeLessThan(5);
  };
  await mapClick(setup.goal, "left");
  await page.waitForTimeout(500);
  // Normal ground input at the camera centre, on the far bank.
  await page
    .locator("#scene")
    .click({ position: { x: 640, y: 360 }, button: "right" });
  await expect(page.locator("#vassal-status")).toContainText("marching");
  await mapClick(setup.camera, "left");
  // Ordinary deselection keeps the Captain/Trainer panel off the crossing.
  await page.locator("#scene").click({ position: { x: 640, y: 80 } });
  await page.mouse.move(640, 80);
  await page.waitForTimeout(4_000);
  if (setup.follow?.[0]) await mapClick(setup.follow[0], "left");
  await page.waitForTimeout(500);
  await capture("middle");
  await page.waitForTimeout(46_000);
  if (setup.follow?.[1]) await mapClick(setup.follow[1], "left");
  await page.waitForTimeout(500);
  await capture("after");
}
