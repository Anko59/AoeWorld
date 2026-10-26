import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { launchMemoryBrowser as boundedInspector } from "./chromium.js";

test("bounded inspector history does not discard application response data", async ({
  playwright,
  baseURL,
}) => {
  const owner = await boundedInspector(playwright.chromium, []);
  try {
    const page = await owner.browser.newPage({ baseURL });
    await page.goto("/health");
    const inspector = await page.context().newCDPSession(page);
    await inspector.send("Network.enable");
    let requestId = "";
    inspector.on("Network.responseReceived", (event) => {
      if (event.response.url.endsWith("/observer-body"))
        requestId = event.requestId;
    });
    await page.route("**/observer-body", (route) =>
      route.fulfill({
        contentType: "text/plain",
        body: "x".repeat(512 * 1024),
      }),
    );
    expect(
      await page.evaluate(async () => {
        const body = await (await fetch("/observer-body")).text();
        Reflect.set(window, "retainedFixtureBody", body);
        return body.length;
      }),
    ).toBe(512 * 1024);
    await expect.poll(() => requestId).not.toBe("");
    await expect(
      inspector.send("Network.getResponseBody", { requestId }),
    ).rejects.toThrow(/evicted from inspector cache/);
    expect(
      await page.evaluate(
        () => String(Reflect.get(window, "retainedFixtureBody")).length,
      ),
    ).toBe(512 * 1024);
    await inspector.detach();
  } finally {
    await owner.close();
  }
});
