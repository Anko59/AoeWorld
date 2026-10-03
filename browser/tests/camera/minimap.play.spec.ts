import { expect, test } from "@playwright/test";
import { gameAssets } from "../game-assets.js";

test("strategic thumbnail is one bounded preview per map and never loads the world", async ({
  page,
  request,
}) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
  const hash = "0b".repeat(32);
  const previews: string[] = [];
  const detailed: string[] = [];
  page.on("request", (event) => {
    if (event.url().includes(`/maps/${hash}/preview`))
      previews.push(event.url());
    if (event.url().includes(`/maps/${hash}/chunks/`))
      detailed.push(event.url());
  });
  await page.route(`**/maps/${hash}/preview`, (route) =>
    route.fulfill({
      json: {
        samples_per_axis: 16,
        cells: Array.from({ length: 256 }, (_, index) => ({
          geographic_height_centimeters: index * 100,
          material: index < 128 ? 0 : 2,
          biome: 9,
          water: index % 16 === 0 ? 1 : 0,
          passable: index % 16 !== 0,
        })),
      },
    }),
  );
  await page.goto("/");
  await expect(page.locator("#connection")).toHaveText("connected");
  // Exercise the DOM end of the production map-change bridge with a controlled preview.
  await page.evaluate(() => {
    const navigation = (
      window as Window & {
        aoeNavigation?: { map(hash: Uint8Array): void };
      }
    ).aoeNavigation;
    if (!navigation) throw new Error("Production navigation bridge missing");
    navigation.map(new Uint8Array(32).fill(11));
    navigation.map(new Uint8Array(32).fill(11));
  });
  await expect(page.locator("#minimap-title")).toContainText(
    "coarse terrain (16×16)",
  );
  await expect(page.locator("#minimap-terrain")).toHaveAttribute(
    "href",
    /^data:image\/png/,
  );
  const camera = await page.locator("#world-position").innerText();
  for (let index = 0; index < 3; index++) {
    await page.getByRole("button", { name: "Show whole map" }).click();
    await expect(page.locator("#minimap")).toHaveClass(/expanded/);
    await page.keyboard.press("Escape");
    await expect(page.locator("#minimap")).not.toHaveClass(/expanded/);
  }
  expect(await page.locator("#world-position").innerText()).toBe(camera);
  expect(previews).toHaveLength(1);
  expect(detailed).toHaveLength(0);
  await page.evaluate(() => {
    const navigation = (
      window as Window & { aoeNavigation?: { map(hash: Uint8Array): void } }
    ).aoeNavigation;
    if (!navigation) throw new Error("Production navigation bridge missing");
    navigation.map(new Uint8Array());
  });
  await expect(page.locator("#minimap-terrain")).not.toHaveAttribute(
    "href",
    /.+/,
  );
  await expect(page.locator("#minimap-title")).toContainText("schematic");
});

test("blur releases held camera keys without stale impulse movement", async ({
  page,
  request,
}) => {
  expect((await request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
  await page.goto("/");
  await expect(page.locator("#connection")).toHaveText("connected");
  await page.mouse.move(600, 300);
  await page.keyboard.down("ArrowRight");
  await page.evaluate(() => {
    window.dispatchEvent(new Event("blur"));
  });
  await page.evaluate(
    () =>
      new Promise<void>((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
      ),
  );
  const camera = await page.locator("#world-position").innerText();
  await page.waitForTimeout(150);
  expect(await page.locator("#world-position").innerText()).toBe(camera);
  await page.keyboard.up("ArrowRight");
});
