import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { forceWebGl } from "./backends.js";
import { fiveVillagerFixture } from "../village/fixtures/village.js";
import { openTab } from "./fixtures/cockpit-tabs.js";

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
  await openTab(page, "build");
  await page.locator('[aria-label="Building type"] [data-id="1"]').click();
  // A strategic label must not intercept the site while placement is armed.
  await page.evaluate(() => {
    const button = document.createElement("button");
    button.className = "steward-card";
    button.textContent = "Castle label above the proposed site";
    button.style.cssText = "left:700px;top:340px";
    document.getElementById("strategic-tokens")?.append(button);
  });
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
  await openTab(page, "build");
  await page.locator(\'[aria-label="Building type"] [data-id="1"]\').click();
  await page.mouse.move(1000, 360);
  await expect(page.locator("#far-building")).toContainText(
    "No available villagers within 64 tiles",
  );
  await page.mouse.click(1000, 360);
  expect(orders).toHaveLength(1);
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
  await openTab(page, "build");
  await page.locator('[aria-label="Building type"] [data-id="1"]').click();
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

type RefreshWindow = Window & {
  buildRefresh(): void;
  buildClicks: number;
  buildIcon: Element | null;
};

test("HUD refresh preserves the Build here icon during a held pointer", async ({
  page,
}) => {
  await page.goto("/");
  await page.evaluate(async () => {
    // @ts-expect-error Served browser module.
    const { initFarBuilding } = await import("/navigation/building.js");
    const panel = document.createElement("div");
    panel.id = "build-refresh-test";
    panel.style.cssText =
      "position:fixed;inset:100px;z-index:99999;background:white";
    document.body.append(panel);
    const build = initFarBuilding(panel);
    const state = new Uint8Array(23).fill(1);
    build.ready(state);
    (window as unknown as RefreshWindow).buildRefresh = () =>
      build.ready(state);
    (window as unknown as RefreshWindow).buildClicks = 0;
    const button = panel.querySelector("button");
    if (!button) throw new Error("Build button missing");
    button.addEventListener(
      "click",
      () => (window as unknown as RefreshWindow).buildClicks++,
    );
    (window as unknown as RefreshWindow).buildIcon = button.firstElementChild;
  });
  const icon = page.locator("#build-refresh-test button > :first-child");
  await icon.hover();
  await page.mouse.down();
  await page.evaluate(() =>
    (window as unknown as RefreshWindow).buildRefresh(),
  );
  expect(
    await page.evaluate(
      () =>
        (window as unknown as RefreshWindow).buildIcon ===
        document.querySelector("#build-refresh-test button")?.firstElementChild,
    ),
  ).toBe(true);
  await page.mouse.up();
  expect(
    await page.evaluate(() => (window as unknown as RefreshWindow).buildClicks),
  ).toBe(1);
});

test("disconnect clears an unacknowledged far build without requiring Welcome or reload", async ({
  page,
}) => {
  await gameAssets(page, true);
  await forceWebGl(page);
  let disconnect: (() => void) | undefined;
  await fiveVillagerFixture(page, (bytes, socket) => {
    if (bytes[0] === 8)
      disconnect = () =>
        socket.close({ code: 1011, reason: "Test connection reset" });
  });
  await page.goto("/?play=1#x=8192&y=8192&zoom=.04");
  await expect(page.locator("#connection")).toHaveText("connected");
  await openTab(page, "build");
  await page.locator(\'[aria-label="Building type"] [data-id="1"]\').click();
  await page.mouse.click(700, 340);
  await expect(page.locator("#far-building")).toContainText("Submitting");
  await page.routeWebSocket("**/game/ws", (socket) =>
    socket.onMessage(() => {}),
  );
  if (!disconnect) throw new Error("Missing unacknowledged build");
  disconnect();
  await expect(page.locator("#far-building")).toContainText("not confirmed");
  await expect(page.locator("#far-building")).not.toContainText("Submitting");
});
