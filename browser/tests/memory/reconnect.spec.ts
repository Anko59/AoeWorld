import { expect } from "@playwright/test";
import { writeFile } from "node:fs/promises";
import { test } from "../../server.js";
import { explorationFixture, memorySnapshot } from "./exploration-fixture.js";

test.use({ screenshot: "off", trace: "off" });

test("reconnects retire their browser callbacks and client references", async ({
  page,
}, testInfo) => {
  test.setTimeout(90_000);
  await explorationFixture(page);
  await page.goto("/?play=1#x=256&y=256");
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-assets",
    "aoe2-local",
  );
  const references: number[] = [];
  for (let index = 0; index < 12; index++) {
    await page.getByRole("button", { name: "Menu", exact: true }).click();
    await page.getByRole("button", { name: "Reconnect to server" }).click();
    await expect(page.locator("#connection")).toHaveText("connected");
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-chunks-pending",
      "0",
    );
    await expect
      .poll(async () => (await memorySnapshot(page)).owned.terrain_requests)
      .toBe(0);
    await page.evaluate(async () => {
      for (let frame = 0; frame < 2; frame++)
        await new Promise<void>((resolve) =>
          requestAnimationFrame(() => resolve()),
        );
    });
    const sample = await memorySnapshot(page);
    references.push(sample.owned.client_references);
  }
  await writeFile(
    testInfo.outputPath("reconnect-references.json"),
    JSON.stringify(references),
  );
  expect(new Set(references).size).toBe(1);
});

test("a stalled browser socket closes before retaining more outgoing messages", async ({
  page,
}) => {
  await explorationFixture(page);
  await page.addInitScript(() => {
    const evidence = { stalled: false, sendsWhileStalled: 0 };
    Object.assign(window, { socketBackpressure: evidence });
    Object.defineProperty(WebSocket.prototype, "bufferedAmount", {
      configurable: true,
      get: () => (evidence.stalled ? 1024 * 1024 : 0),
      set: () => {},
    });
    const send = WebSocket.prototype.send;
    WebSocket.prototype.send = function (data) {
      if (evidence.stalled) evidence.sendsWhileStalled++;
      send.call(this, data);
    };
  });
  await page.goto("/?play=1#x=256&y=256");
  await expect(page.locator("#connection")).toHaveText("connected");
  await page.evaluate(() => {
    (
      window as unknown as { socketBackpressure: { stalled: boolean } }
    ).socketBackpressure.stalled = true;
    location.hash = "x=1200&y=1200";
  });
  await expect(page.locator("#connection")).toHaveText(/reconnect/);
  expect(
    await page.evaluate(
      () =>
        (
          window as unknown as {
            socketBackpressure: { sendsWhileStalled: number };
          }
        ).socketBackpressure.sendsWhileStalled,
    ),
  ).toBe(0);
});
