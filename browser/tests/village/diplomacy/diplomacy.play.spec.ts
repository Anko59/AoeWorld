import { expect, type Page } from "@playwright/test";
import { fallbackTest as test } from "../../rendering/backends.js";
import { gameAssets } from "../../game-assets.js";
import { freshHud } from "../fixtures/resources.js";

type Actor = {
  kind: string;
  owned: boolean;
  x: number;
  hp: number;
  maxHp: number;
};

async function visitor(page: Page) {
  const value = await page.locator("#scene").getAttribute("data-actors");
  return (JSON.parse(value ?? "[]") as Actor[]).find(
    (actor) => actor.kind === "Militia" && !actor.owned,
  );
}

test.use({ video: { mode: "on", size: { width: 1280, height: 720 } } });

test("two seats agree an alliance, share market locations and receive tribute", async ({
  browser,
  page,
  request,
}) => {
  test.setTimeout(95_000);
  await gameAssets(page, true);
  expect(
    (await request.post("/maps/reset?scenario=diplomacy-pair")).status(),
  ).toBe(204);
  await page.goto("/?play=1#x=108&y=104&zoom=.48");
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-connected",
    "true",
  );
  await freshHud(page);

  const other = await browser.newPage();
  try {
    await gameAssets(other, true);
    await other.goto(page.url());
    await expect(other.locator("#scene")).toHaveAttribute(
      "data-connected",
      "true",
    );
    await freshHud(other);
    await page.locator("#unpause-game").click();
    await page.locator("#scoreboard summary").click();
    await expect(page.locator("#scoreboard-players")).toContainText("Lyon");

    const stance = page.getByRole("combobox", { name: "Stance toward Lyon" });
    await page.getByRole("button", { name: "Diplomacy", exact: true }).click();
    await expect(stance).toHaveValue("0");
    await expect(page.locator("#minimap-allied-markets")).toHaveAttribute(
      "d",
      "",
    );
    await stance.selectOption({ label: "Ally" });
    await expect(page.getByText("Offer sent", { exact: true })).toBeVisible();

    await other.getByRole("button", { name: "Diplomacy", exact: true }).click();
    await other.getByRole("button", { name: "Accept alliance" }).click();
    await expect(stance).toHaveValue("2");
    await expect(page.locator("#minimap-allied-markets")).not.toHaveAttribute(
      "d",
      "",
    );

    const sender = page.locator("#stockpile-wood");
    const receiver = other.locator("#stockpile-wood");
    const beforeSender = Number(await sender.textContent());
    const beforeReceiver = Number(await receiver.textContent());
    await page.locator("#tribute-resource").selectOption({ label: "Wood" });
    await page.getByRole("button", { name: "Send tribute to Lyon" }).click();
    await expect(page.locator("#diplomacy-result")).toHaveText(
      "Sent 100 wood to Lyon.",
    );
    await expect(sender).toHaveText(String(beforeSender - 130));
    await expect(receiver).toHaveText(String(beforeReceiver + 100));

    await page.getByRole("button", { name: "Close diplomacy" }).click();
    await expect.poll(() => visitor(page), { timeout: 30_000 }).toBeDefined();
    const first = await visitor(page);
    if (!first) throw new Error("The visiting militia left the view");
    await expect
      .poll(
        async () => {
          const unit = await visitor(page);
          return unit && unit.hp === unit.maxHp ? first.x - unit.x : 0;
        },
        { timeout: 30_000 },
      )
      .toBeGreaterThan(100);
  } finally {
    await other.close();
  }
});
