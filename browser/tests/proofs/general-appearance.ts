import { expect, type Page } from "@playwright/test";
import { writeFile } from "node:fs/promises";

type Actor = {
  id: number;
  kind: string;
  x: number;
  y: number;
  hp: number;
  task: string;
  owned: boolean;
};

/** Close, real-time encounter: no injected poses, damage or animation clocks. */
export async function generalAppearanceProof(page: Page, url: string) {
  expect(
    (
      await page.request.post(`${url}maps/reset?scenario=general-appearance`)
    ).status(),
  ).toBe(204);
  await page.goto(`${url}?play=1#x=135.5&y=128.5&zoom=0.65&renderer=webgpu`);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    "webgpu",
  );
  const actors = async (): Promise<Actor[]> =>
    JSON.parse(
      (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
    );
  const general = async () =>
    (await actors()).find((a) => a.kind === "General");
  const capture = async (name: string) => {
    await page.screenshot({
      path: `../.cache/ui/general-appearance-${name}.png`,
    });
    await writeFile(
      `../.cache/ui/general-appearance-${name}.json`,
      JSON.stringify(
        {
          actors: await actors(),
          camera: await page
            .locator("#minimap")
            .evaluate((e) => ({ ...e.dataset })),
        },
        null,
        2,
      ),
    );
  };
  await expect.poll(async () => (await general())?.task).toBe("Idle");
  await capture("idle");
  const hero = await general();
  const target = (await actors()).find(
    (a) => a.kind === "BatteringRam" && !a.owned,
  );
  if (!hero || !target) throw new Error("Encounter actors missing");
  await page.mouse.click(hero.x, hero.y);
  await expect(page.locator("#selection")).toHaveAttribute(
    "data-kind",
    "General",
  );
  await page.mouse.click(target.x, target.y, { button: "right" });
  await expect.poll(async () => (await general())?.task).toBe("Attacking");
  await page.waitForTimeout(2000);
  await capture("walk");
  await expect
    .poll(
      async () =>
        (await actors()).some((a) => a.id === target.id && a.hp < target.hp),
      { timeout: 30_000 },
    )
    .toBe(true);
  await page.evaluate(() => {
    location.hash = "x=142&y=129&zoom=1.2&renderer=webgpu";
  });
  await expect(page.locator("#minimap")).toHaveAttribute("data-zoom", "1.2");
  await capture("attack");
  await expect
    .poll(async () => (await general())?.hp, {
      timeout: 60_000,
      intervals: [100],
    })
    .toBe(0);
  await capture("death");
  await page.waitForTimeout(2000);
  await capture("decay");
}
