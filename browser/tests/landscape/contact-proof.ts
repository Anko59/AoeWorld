import { expect, type Page } from "@playwright/test";
import { freshHud } from "../village/fixtures/resources.js";

type Actor = { id: number; kind: string; x: number; y: number; hp: number };
export async function contactProof(page: Page, home: { x: number; y: number }) {
  const actors = async (): Promise<Actor[]> =>
    JSON.parse(
      (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
    ) as Actor[];
  await page.evaluate(({ x, y }) => {
    location.hash = `x=${x}&y=${y}&zoom=.5`;
  }, home);
  await freshHud(page);
  const worker = (await actors()).find(
    (a) => a.kind === "Villager" && a.y < 480,
  );
  if (!worker) throw new Error("No builder for farm foundation proof");
  await page.mouse.click(worker.x, worker.y);
  await page.locator("#build-menu").click();
  const place = async (id: string) => {
    await page.locator(id).click();
    for (const [x, y] of [
      [1000, 360],
      [900, 400],
      [850, 280],
      [450, 400],
      [450, 280],
      [640, 470],
    ]) {
      await page.mouse.move(x ?? 0, y ?? 0);
      await freshHud(page);
      if (
        (await page.locator("#scene").getAttribute("data-placement-valid")) ===
        "true"
      ) {
        await page.mouse.click(x ?? 0, y ?? 0);
        return;
      }
    }
    throw new Error(`No legal site for ${id}`);
  };
  await place("#build-mill");
  await expect(page.locator("#build-farm")).toBeEnabled({ timeout: 70_000 });
  await place("#build-farm");
  await page.locator("#build-menu").click();
  await page.locator("#stop-units").click();
  await freshHud(page);
  const farm = (await actors()).find((a) => a.kind === "Farm");
  const scout = (await actors()).find((a) => a.kind === "Scout");
  if (!farm || !scout) throw new Error("Farm foundation or scout missing");
  const world = (p: { x: number; y: number }) => {
    const dx = (p.x - 640) / 32,
      dy = (p.y - 360) / 16;
    return { x: home.x + (dx + dy) / 2, y: home.y + (dy - dx) / 2 };
  };
  const foundation = world(farm);
  await page.mouse.click(scout.x, scout.y);
  await expect(page.locator("#selection-name")).toContainText("Scout");
  // Finish with a straight south-east segment: DE decay row 2 reaches forty
  // source pixels below the hotspot, exceeding the old fixed clearance.
  for (const [x, y] of [
    [1030, 440],
    [1130, 490],
  ]) {
    await page.mouse.click(x ?? 0, y ?? 0, { button: "right" });
    await expect(page.locator("#unit-state")).not.toHaveText("unit idle");
    await expect(page.locator("#unit-state")).toHaveText("unit idle", {
      timeout: 30_000,
    });
  }
  await freshHud(page);
  const at = (await actors()).find((a) => a.id === scout.id);
  if (!at) throw new Error("Scout disappeared before death");
  const corpse = world(at);
  await page.keyboard.press("Delete");
  await expect
    .poll(async () => (await actors()).find((a) => a.id === scout.id)?.hp ?? 0)
    .toBe(0);
  await page.waitForTimeout(6000); // Finish the native death sequence before the decay proof.
  await page.keyboard.press("Pause");
  await expect(page.locator("body")).toHaveAttribute("data-paused", "true");
  return { corpse, foundation };
}
