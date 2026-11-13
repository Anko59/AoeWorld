import { expect, type Page } from "@playwright/test";
import { writeFile } from "node:fs/promises";

type Actor = { id: number; kind: string; x: number; y: number; owned: boolean };

/** Local before/after footage: real training, micro and queued client orders. */
export async function generalsControlProof(
  page: Page,
  url: string,
  side: "before" | "after",
) {
  expect(
    (
      await page.request.post(`${url}maps/reset?scenario=general-army`)
    ).status(),
  ).toBe(204);
  await page.goto(`${url}?play=1#x=106&y=128&zoom=0.5&renderer=webgl2`);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-village-art",
    "native",
  );
  const actors = async (): Promise<Actor[]> =>
    JSON.parse(
      (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
    );
  const select = async (kind: string, group = false) => {
    // A newly trained unit's native sprite can arrive after its HUD record.
    // Click its body using a fresh rendered position, then wait for selection.
    await expect
      .poll(async () => {
        const actor = (await actors()).find((a) => a.kind === kind && a.owned);
        if (!actor) return "";
        const y = actor.y - (kind === "Castle" ? 0 : 8);
        if (group) await page.mouse.dblclick(actor.x, y);
        else await page.mouse.click(actor.x, y);
        return page.locator("#selection").getAttribute("data-kind");
      })
      .toBe(kind);
  };
  const caption = async (text: string) => {
    await page.evaluate(
      ({ side, text }) => {
        let label = document.getElementById("generals-proof-caption");
        if (!label) {
          label = document.createElement("div");
          label.id = "generals-proof-caption";
          label.style.cssText =
            "position:fixed;top:8px;left:240px;z-index:99999;background:#101820ee;color:white;padding:12px;pointer-events:none;font:18px sans-serif";
          document.body.append(label);
        }
        label.textContent = `${side.toUpperCase()}: ${text}`;
      },
      { side, text },
    );
  };
  const roster = async () => {
    const status = (await page.locator("#steward-status").textContent()) ?? "";
    return Number(/(\d+) soldiers/.exec(status)?.[1] ?? NaN);
  };
  await caption("Train a General and take one soldier out of his army");
  await select("Castle");
  await page.locator("#train-general").click();
  await expect
    .poll(async () => (await actors()).some((a) => a.kind === "General"), {
      timeout: 25_000,
    })
    .toBe(true);
  await select("General");
  const original = await roster();
  expect(original).toBeGreaterThan(0);
  await select("Militia");
  await page.locator("#stop-units").click();
  await select("General");
  await expect.poll(roster).toBe(original - 1);
  await caption("The soldier stays idle for more than 300 ticks");
  await page.waitForTimeout(16_000);
  await page.locator("#stop-units").click();
  await page.waitForTimeout(1500);
  const afterOrder = await roster();
  await caption(
    `General Stop: ${afterOrder} soldiers (started with ${original})`,
  );
  await page.screenshot({
    path: `../.cache/showcase/generals-${side}-micro.png`,
  });
  await page.waitForTimeout(2500);

  await caption("General: move, then Shift-right-click another destination");
  await page.mouse.click(1000, 280, { button: "right" });
  await page.keyboard.down("Shift");
  await page.mouse.click(450, 280, { button: "right" });
  await page.keyboard.up("Shift");
  await page.waitForTimeout(500);
  const generalMarkers = await page.locator("#waypoint-markers g").count();
  await page.screenshot({
    path: `../.cache/showcase/generals-${side}-queue.png`,
  });
  await page.waitForTimeout(2500);
  await page.locator("#stop-units").click();

  await select("Militia", true);
  await caption("Attack-move: first destination, then two Shift-left-clicks");
  await page.locator("#attack-move-units").click();
  await page.mouse.click(1050, 340);
  await page.locator("#attack-move-units").click();
  await page.keyboard.down("Shift");
  await page.mouse.click(450, 300);
  await page.mouse.click(950, 400);
  await page.keyboard.up("Shift");
  await page.waitForTimeout(500);
  const attackMarkers = await page.locator("#waypoint-markers g").count();
  const mode = await page.locator("#scene").getAttribute("data-order-mode");
  await page.screenshot({
    path: `../.cache/showcase/generals-${side}-attack-queue.png`,
  });
  await page.waitForTimeout(4000);
  await writeFile(
    `../.cache/showcase/generals-${side}.json`,
    JSON.stringify(
      { original, afterOrder, generalMarkers, attackMarkers, mode },
      null,
      2,
    ),
  );
  const video = page.video();
  await page.close();
  await video?.saveAs(`../.cache/showcase/generals-${side}.webm`);
  if (side === "after") {
    expect(afterOrder).toBe(original - 1);
    expect(generalMarkers).toBe(2);
    expect(attackMarkers).toBeGreaterThanOrEqual(3);
    expect(mode).toBe("attack-move");
  }
}
