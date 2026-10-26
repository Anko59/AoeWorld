import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { gameAssets } from "../../game-assets.js";
import { fiveVillagerFixture } from "../fixtures/village.js";

for (const [kind, unitKind] of [
  ["Militia", 2],
  ["Knight", 12],
] as const) {
  test(`${kind} groups select four formations and send one queued group move`, async ({
    page,
  }) => {
    const orders: Buffer[] = [];
    await gameAssets(page);
    await fiveVillagerFixture(
      page,
      (bytes) => {
        if (bytes[0] === 29) orders.push(bytes);
      },
      () => 0,
      false,
      true,
      unitKind,
    );
    await page.goto("/?play=1");
    await expect(page.locator("#connection")).toHaveText("connected", {
      timeout: 30000,
    });
    const canvas = page.locator("#scene");
    await expect
      .poll(async () =>
        (await canvas.getAttribute("data-actors"))?.includes(kind),
      )
      .toBe(true);
    const first = await canvas.evaluate(
      (c, kind) =>
        (
          JSON.parse(c.getAttribute("data-actors") ?? "[]") as {
            kind: string;
            x: number;
            y: number;
          }[]
        ).find((a) => a.kind === kind),
      kind,
    );
    if (!first) throw new Error("Military fixture missing");
    await page.mouse.dblclick(first.x, first.y);
    await expect(page.locator("#selection")).toHaveAttribute("data-count", "5");
    for (const name of ["line", "box", "staggered", "flank"]) {
      const button = page.locator(`#formation-${name}`);
      await expect(button).toBeVisible();
      await button.click();
      await expect(button).toHaveAttribute("aria-pressed", "true");
      await expect(
        page.locator('.formation-command[aria-pressed="true"]'),
      ).toHaveCount(1);
    }
    await expect.poll(() => orders.length).toBe(4);
    for (const order of orders) expect(order[9]).toBe(0);
    orders.length = 0;
    // Clear ground above the native TC wings and the fixture sheep/scout.
    // The former point hit native actor art and correctly chose interaction.
    await page.mouse.click(1100, 200, { button: "right" });
    await expect.poll(() => orders.length).toBe(1);
    expect(orders[0]?.[2]).toBe(5);
    expect(orders[0]?.[8]).toBe(3);
    expect(orders[0]?.at(-2)).toBe(0);
    await page.keyboard.down("Shift");
    await page.mouse.click(1150, 220, { button: "right" });
    await page.keyboard.up("Shift");
    await expect.poll(() => orders.length).toBe(2);
    expect(orders[1]?.at(-2)).toBe(1);
    await page.locator("#attack-move-units").click();
    await page.mouse.click(1120, 230, { button: "right" });
    await expect.poll(() => orders.length).toBe(3);
    expect(orders[2]?.at(-1)).toBe(1);
    await page.locator("#attack-move-units").click();
    await page.locator("#stop-units").click();
    await expect(canvas).toHaveAttribute("data-order-mode", "");
    await page.mouse.click(1100, 200, { button: "right" });
    await expect.poll(() => orders.length).toBe(4);
    expect(orders[3]?.at(-1)).toBe(0);

    await page.keyboard.press("Escape");
    await expect(page.locator("#formation-line")).toBeHidden();
  });
}
