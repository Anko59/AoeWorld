import { expect, test, type Page } from "@playwright/test";
import { gameAssets } from "../game-assets.js";
import { syntheticChunk } from "../synthetic-map.js";

// Browser feedback only: synthetic immutable terrain and server messages do not
// qualify real geographic maps, simulation behavior or hardware performance.
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

test("returning during backoff restores the unresolved terrain warning until recovery", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await feedbackWorld(page, 16384, 8192.5, 2);
  await page.addInitScript(() => {
    const original = window.fetch.bind(window);
    let failed = false;
    window.fetch = (input, init) => {
      if (!failed && String(input).endsWith("/chunks/256/256")) {
        failed = true;
        return new Promise<Response>((_, reject) => {
          Object.assign(window, {
            failTerrain: () =>
              reject(new TypeError("return view terrain probe")),
          });
        });
      }
      return original(input, init);
    };
  });
  let retries = 0;
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route(`**/maps/${hash}/chunks/256/256`, async (route) => {
    retries++;
    await held;
    await route.fulfill({ json: syntheticChunk(256, 256) });
  });
  const status = page.locator("#connection");
  const warning = "Some terrain could not be loaded; retrying.";
  try {
    await page.goto("/?play=1");
    await expect(page.locator("#playground")).toHaveAttribute(
      "data-assets",
      "aoe2-local",
    );
    await page.locator("#recenter").click();
    await expect(page.locator("#world-position")).toHaveText("8192.5, 8192.5");
    await expect
      .poll(() => page.evaluate(() => "failTerrain" in window))
      .toBe(true);
    await page.getByRole("button", { name: "Show whole map" }).click();
    const box = await page.locator("#minimap-map").boundingBox();
    if (!box) throw new Error("Missing minimap");
    const side = Math.min(box.width, box.height);
    const logged = page.waitForEvent("console", {
      predicate: (message) =>
        message.text().includes("return view terrain probe"),
    });
    await page.evaluate(() =>
      (window as unknown as { failTerrain(): void }).failTerrain(),
    );
    await logged;
    await expect(status).toHaveText(warning);
    await page.mouse.click(
      box.x + (box.width - side) / 2 + side * 0.75,
      box.y + (box.height - side) / 2 + side * 0.25,
    );
    await expect(page.locator("#world-position")).toHaveText("12288.0, 4096.0");
    await expect(status).toHaveText("connected");
    await page.getByRole("button", { name: "Close map overview" }).click();
    await page.locator("#recenter").click();
    await expect(page.locator("#world-position")).toHaveText("8192.5, 8192.5");
    expect(retries).toBe(0); // Returned inside the unchanged five-second backoff.
    await expect(status).toHaveText(warning);
    await expect.poll(() => retries, { timeout: 15000 }).toBe(1);
    await expect(status).toHaveText(warning); // Retry response is still held.
    release();
    await expect(status).toHaveText("connected");
  } finally {
    release();
  }
});

function readUnsigned(bytes: Buffer, offset = 1) {
  let value = 0;
  let scale = 1;
  for (; offset < bytes.length; offset++) {
    const byte = bytes[offset] ?? 0;
    value += (byte & 127) * scale;
    if (byte < 128) return { value, next: offset + 1 };
    scale *= 128;
  }
  throw new Error("Incomplete synthetic wire integer");
}

async function feedbackWorld(
  page: Page,
  size = 16384,
  start = 8192.5,
  maximumHeight = 2,
) {
  await gameAssets(page);
  let revision = 0;
  const position = unsigned(start * 1024 * 2);
  await page.routeWebSocket("**/game/ws", (socket) => {
    socket.onMessage((message) => {
      const bytes =
        typeof message === "string" ? Buffer.from(message) : message;
      if (bytes[0] === 0) {
        socket.send(
          Buffer.from([
            0,
            2,
            1,
            1,
            ...Array<number>(32).fill(2),
            0,
            ...unsigned(size * 2),
            ...unsigned(size * 2),
            ...unsigned(1024),
            20,
            0,
            1,
            0,
          ]),
        );
      }
      if (bytes[0] === 1) {
        revision = readUnsigned(bytes).value;
        socket.send(
          Buffer.from([
            1,
            ...unsigned(revision),
            0,
            1,
            1,
            0,
            ...position,
            ...position,
            0,
            0,
            4,
            1, // Scout
            0, // Idle
            0, // No carried resource
            0, // No gathering job
            0, // Players
            0, // Buildings
          ]),
        );
        socket.send(Buffer.from([6, ...unsigned(revision), 0, 0, 0]));
      }
    });
  });
  await page.route(`**/maps/${hash}/height-bounds`, (route) =>
    route.fulfill({
      json: {
        content_hash: hash,
        minimum_height_level: 0,
        maximum_height_level: maximumHeight,
      },
    }),
  );
  await page.route(`**/maps/${hash}/chunks/*/*`, (route) => {
    const [x = 0, y = 0] = route
      .request()
      .url()
      .split("/")
      .slice(-2)
      .map(Number);
    return route.fulfill({ json: syntheticChunk(x, y) });
  });
}
