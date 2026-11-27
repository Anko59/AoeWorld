import { expect } from "@playwright/test";
import { test } from "../../../server.js";

test("a rate-limited minute is retried with its original data before the next minute", async ({
  page,
}) => {
  await page.route("**/frame-telemetry-test", (route) =>
    route.fulfill({
      contentType: "text/html",
      body: '<div id="playground" data-renderer="webgl2"></div>',
    }),
  );
  const sent: { sequence: [number, number]; units_in_view: number }[] = [];
  await page.route("**/client/perf", async (route) => {
    sent.push(route.request().postDataJSON());
    await route.fulfill({ status: sent.length === 1 ? 429 : 204 });
  });
  await page.goto("/frame-telemetry-test");
  const publish = await page.evaluateHandle(async () => {
    // @ts-expect-error Served browser module.
    const { initFrameTelemetry } = await import("/dev/frame-telemetry.js");
    const telemetry = initFrameTelemetry();
    let now = 1;
    performance.now = () => now;
    telemetry.session("seat");
    return (units: number) => {
      for (const delta of [0, 60_010]) {
        now += delta;
        telemetry.mark(0);
        telemetry.mark(21);
      }
      if (!telemetry.view(0, 0)) throw new Error("No minute to report");
      telemetry.publish(units, 0, 0, 0, 0, 0, 0, 0);
    };
  });
  const refused = page.waitForResponse(
    (response) =>
      response.url().endsWith("/client/perf") && response.status() === 429,
  );
  await page.evaluate((submit) => submit(12), publish);
  await refused;
  await page.evaluate((submit) => submit(15), publish);

  await expect.poll(() => sent, { timeout: 20_000 }).toHaveLength(3);
  const [first, retry, next] = sent;
  expect(retry).toEqual(first);
  expect(sent.map((report) => report.units_in_view)).toEqual([12, 12, 15]);
  expect(next?.sequence[0]).toBeGreaterThan(first?.sequence[0] ?? Infinity);
});
