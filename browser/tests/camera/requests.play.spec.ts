import { expect, test } from "@playwright/test";
import { gameAssets } from "../game-assets.js";
import { syntheticChunk } from "../synthetic-map.js";

const hash = "02".repeat(32);

function unsigned(value: number): number[] {
  const bytes: number[] = [];
  do {
    const digit = value % 128;
    value = Math.floor(value / 128);
    bytes.push(digit + (value ? 128 : 0));
  } while (value);
  return bytes;
}

test("camera demand aborts a full old window without backoff or exceeding 64", async ({
  page,
}) => {
  // Transport and terrain are synthetic. This tests browser cancellation, not
  // real-source fidelity, RTS traversal or production hardware performance.
  await gameAssets(page);
  await page.addInitScript(() => {
    const original = window.fetch.bind(window);
    const pending = new Set<AbortSignal | undefined>();
    const evidence = { maximumLive: 0, aborts: 0 };
    Object.assign(window, { terrainRequestEvidence: evidence });
    window.fetch = (input, init) => {
      const url =
        typeof input === "string"
          ? input
          : input instanceof URL
            ? input.href
            : input.url;
      if (!url.includes("/chunks/")) return original(input, init);
      const signal = init?.signal ?? undefined;
      pending.add(signal);
      signal?.addEventListener("abort", () => evidence.aborts++, {
        once: true,
      });
      evidence.maximumLive = Math.max(
        evidence.maximumLive,
        [...pending].filter((value) => !value?.aborted).length,
      );
      return original(input, init).finally(() => pending.delete(signal));
    };
  });
  const center = unsigned((8192 * 1024 + 512) * 2);
  const welcome = Buffer.from([
    0,
    8,
    1,
    1,
    ...Array<number>(32).fill(2),
    0,
    ...unsigned(32768),
    ...unsigned(32768),
    ...unsigned(1024),
    20,
    0,
    1,
    0,
  ]);
  await page.routeWebSocket("**/game/ws", (socket) => {
    socket.onMessage((message) => {
      const bytes =
        typeof message === "string" ? Buffer.from(message) : message;
      if (bytes[0] === 0) socket.send(welcome);
      if (bytes[0] !== 0 && bytes[0] !== 1) return;
      const revision: number[] = [];
      if (bytes[0] === 1) {
        for (let index = 1; index < bytes.length; index++) {
          const byte = bytes[index] ?? 0;
          revision.push(byte);
          if (byte < 128) break;
        }
      } else revision.push(0);
      socket.send(
        Buffer.from([
          1,
          ...revision,
          0,
          1,
          1,
          0,
          ...center,
          ...center,
          0,
          0,
          4,
        ]),
      );
    });
  });
  await page.route(`**/maps/${hash}/height-bounds`, (route) =>
    route.fulfill({
      json: {
        content_hash: hash,
        minimum_height_level: -32000,
        maximum_height_level: 32000,
      },
    }),
  );
  let holding = true;
  let held = 0;
  let served = 0;
  let release!: () => void;
  const released = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route(`**/maps/${hash}/chunks/*/*`, async (route) => {
    if (holding) {
      held++;
      await released;
      // These deliberately held requests have already been aborted by the client;
      // teardown can close their intercepted routes before this cleanup executes.
      await route.abort("aborted").catch(() => undefined);
      return;
    }
    const coordinates = route.request().url().split("/").slice(-2).map(Number);
    const x = coordinates[0];
    const y = coordinates[1];
    if (x === undefined || y === undefined)
      throw new Error("missing chunk coordinate");
    await route.fulfill({ json: syntheticChunk(x, y) });
    served++;
  });
  try {
    await page.goto("/");
    await expect(page.locator("#connection")).toHaveText("connected");
    await expect.poll(() => held).toBe(64);
    holding = false;
    await page.getByRole("button", { name: "Show whole map" }).click();
    const bounds = await page.locator("#minimap-map").boundingBox();
    if (!bounds) throw new Error("minimap missing");
    const side = Math.min(bounds.width, bounds.height);
    await page.mouse.click(
      bounds.x + (bounds.width - side) / 2 + side * 0.75,
      bounds.y + (bounds.height - side) / 2 + side * 0.25,
    );
    // Crucially, old requests stay unreleased while the new camera gets terrain.
    await expect.poll(() => served).toBeGreaterThan(0);
    await expect(page.locator("#connection")).toHaveText("connected");
    await expect(page.locator("#terrain-cache")).not.toContainText(
      "(0 / 512 chunks)",
    );
    const evidence = await page.evaluate(
      () =>
        (
          window as unknown as {
            terrainRequestEvidence: { maximumLive: number; aborts: number };
          }
        ).terrainRequestEvidence,
    );
    expect(evidence.aborts).toBeGreaterThanOrEqual(64);
    expect(evidence.maximumLive).toBe(64);
    release();
    await page.getByRole("button", { name: "Close map overview" }).click();
    await page.keyboard.press("Home");
    await expect
      .poll(async () =>
        (await page.locator("#world-position").innerText())
          .split(",")
          .map(Number),
      )
      .toEqual([8192.5, 8192.5]);
    await expect(page.locator("#connection")).toHaveText("connected");
  } finally {
    release();
  }
});
