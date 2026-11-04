import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { forceWebGl } from "./backends.js";
import { fiveVillagerFixture } from "../village/fixtures/village.js";

test("far construction previews the footprint and reports only server acknowledgements", async ({
  page,
}) => {
  await gameAssets(page, true);
  await forceWebGl(page);
  const orders: number[][] = [];
  let acknowledge: (() => void) | undefined;
  await fiveVillagerFixture(page, (bytes, socket) => {
    if (bytes[0] !== 8) return;
    orders.push([...bytes]);
    acknowledge = () => socket.send(Buffer.from([3, bytes[1] ?? 0, 4, 0]));
  });
  await page.goto("/?play=1#x=8192&y=8192&zoom=.04");
  await expect(page.locator("#connection")).toHaveText("connected");
  await page.getByLabel("Building type").selectOption("1");
  await page.getByRole("button", { name: "Build here", exact: true }).click();
  await page.mouse.move(700, 340);
  await expect(page.locator("#far-footprint")).toBeVisible();
  await expect(page.locator("#far-footprint")).toHaveAttribute(
    "data-valid",
    "true",
  );
  await page.mouse.click(700, 340);
  await expect.poll(() => orders.length).toBe(1);
  await expect(page.locator("#far-building")).toContainText("Submitting");
  await expect(page.locator("#far-building")).not.toContainText("Accepted");
  if (!acknowledge) throw new Error("Missing build order");
  acknowledge();
  await expect(page.locator("#far-building")).toContainText("Too many orders");
  await expect(page.locator("#scene")).toHaveAttribute("data-lod", "markers");
  // Choosing a site over the Town Center refuses locally, with a red reason.
  await page.getByRole("button", { name: "Build here", exact: true }).click();
  await page.mouse.move(640, 360);
  await expect(page.locator("#far-footprint")).toHaveAttribute(
    "data-valid",
    "false",
  );
  await expect(page.locator("#far-building p")).toHaveAttribute(
    "data-invalid",
    "true",
  );
  await page.mouse.click(640, 360);
  expect(orders).toHaveLength(1);
  await page.keyboard.press("Escape");
  await expect(page.locator("#far-footprint")).toBeHidden();
});

test("far construction spends real stock through the ordinary validated build order", async ({
  page,
  request,
}) => {
  test.setTimeout(60_000);
  expect(
    (await request.post("/maps/reset?scenario=feudal-military")).status(),
  ).toBe(204);
  await forceWebGl(page);
  await gameAssets(page);
  await page.goto("/?play=1#x=128&y=128&zoom=.04");
  await expect(page.locator("#connection")).toHaveText("connected");
  await page.getByLabel("Building type").selectOption("1");
  await page.getByRole("button", { name: "Build here", exact: true }).click();
  await page.mouse.move(650, 380);
  await expect(page.locator("#far-footprint")).toHaveAttribute(
    "data-valid",
    "true",
  );
  await page.mouse.click(650, 380);
  await expect(page.locator("#far-building")).toContainText(
    "Accepted 1 building order",
  );
  await expect(page.locator("#stockpile-wood")).toHaveText("9975");
  await expect(page.locator("#scene")).toHaveAttribute("data-lod", "markers");
});
