import { expect, type Page, type WebSocketRoute } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { syntheticChunk } from "../synthetic-map.js";
import { scoutCombat } from "../wire/combat.js";

// Browser feedback only: synthetic immutable terrain and server messages do not
// qualify real geographic maps, simulation behavior or hardware performance.
const hash = "02".repeat(32);
const tree = [8194, 8192] as const;
const treeId = tree[1] * 524288 + tree[0] * 2;

function unsigned(value: number): number[] {
  const bytes: number[] = [];
  do {
    const digit = value % 128;
    value = Math.floor(value / 128);
    bytes.push(digit + (value ? 128 : 0));
  } while (value);
  return bytes;
}

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

function chunkWithTree(x: number, y: number) {
  const chunk = syntheticChunk(x, y);
  if (x !== 256 || y !== 256) return chunk;
  const planted = syntheticChunk(
    x,
    y,
    (tx, ty) => tx === tree[0] && ty === tree[1],
  );
  const bytes = Buffer.from(chunk.payload_hex, "hex");
  bytes.writeUInt16LE(1, 3);
  // Preserve the existing open/water terrain fixture; add its one wood node.
  const resource = Buffer.from(planted.payload_hex, "hex").subarray(
    7 + 1024 * 35,
  );
  return {
    ...chunk,
    payload_hex: Buffer.concat([bytes, resource]).toString("hex"),
  };
}

async function feedbackWorld(
  page: Page,
  size = 16384,
  start = 8192.5,
  maximumHeight = 2,
) {
  await gameAssets(page);
  let active: WebSocketRoute | undefined;
  let revision = 0;
  let result = 6; // RejectedInvalidDestination, the server's resource-blocked result.
  const orders: number[] = [];
  const position = unsigned(start * 1024 * 2);
  await page.routeWebSocket("**/game/ws", (socket) => {
    active = socket;
    socket.onMessage((message) => {
      const bytes =
        typeof message === "string" ? Buffer.from(message) : message;
      if (bytes[0] === 0) {
        socket.send(
          Buffer.from([
            0,
            4,
            1,
            1,
            ...Array<number>(32).fill(2),
            0,
            ...unsigned(size * 2),
            ...unsigned(size * 2),
            ...unsigned(1024),
            20,
            1,
            ...Array<number>(24).fill(1),
          ]),
        );
      }
      if (bytes[0] === 1) {
        revision = readUnsigned(bytes).value;
        socket.send(disabledFog(unsigned(revision)));
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
            ...scoutCombat,
            0, // Players
            0, // Buildings
          ]),
        );
        socket.send(Buffer.from([5, ...unsigned(revision), 0, 0, 0, 0]));
      }
      if (bytes[0] === 2) {
        const sequence = readUnsigned(bytes).value;
        orders.push(sequence);
        socket.send(Buffer.from([3, ...unsigned(sequence), result, 0]));
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
    return route.fulfill({ json: chunkWithTree(x, y) });
  });
  return {
    orders,
    rejectWith(value: number) {
      result = value;
    },
    deplete() {
      if (!active || revision === 0)
        throw new Error("No subscribed controller");
      active.send(
        Buffer.from([
          5,
          ...unsigned(revision),
          1,
          0,
          1,
          1,
          ...unsigned(treeId),
          0,
          0, // Current ResourceState snapshot-page option (None).
        ]),
      );
    },
  };
}

async function loadedWorld(page: Page) {
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-assets",
    "aoe2-local",
  );
  await expect
    .poll(async () => {
      await page
        .getByRole("button", { name: "Center on primary unit" })
        .click();
      return page.locator("#world-position").innerText();
    })
    .toBe("8192.5, 8192.5");
  const canvas = page.locator("#scene");
  const box = await canvas.boundingBox();
  if (!box) throw new Error("Missing play canvas");
  const center = { x: box.width / 2, y: box.height / 2 };
  await canvas.hover({ position: center });
  await expect(page.locator("#tile-inspection")).toContainText(
    "tile 8192, 8192:",
  );
  await expect(page.locator("#tile-inspection")).toContainText("passable");
  return { canvas, center, target: { x: center.x + 128, y: center.y + 64 } };
}

async function transientFailure(page: Page) {
  await feedbackWorld(page);
  await page.addInitScript(() => {
    const fetch = window.fetch.bind(window);
    let failed = false;
    window.fetch = async (input, init) => {
      if (!failed && String(input).endsWith("/chunks/256/256")) {
        failed = true;
        throw new TypeError("Failed to fetch\n    at terrainFeedbackProbe");
      }
      return fetch(input, init);
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
    await route.fulfill({ json: chunkWithTree(256, 256) });
  });
  return { release, retries: () => retries };
}
test("failed terrain shows a short single-line sentence and logs details", async ({
  page,
}) => {
  const failure = await transientFailure(page);
  const consoleDetails: string[] = [];
  page.on("console", (message) => consoleDetails.push(message.text()));
  try {
    await page.goto("/?play=1");
    const status = page.locator("#connection");
    await expect(status).toContainText(/terrain.*(?:load|unavailable).*retry/i);
    const text = await status.textContent();
    expect(text).not.toMatch(
      /\r|\n|JsValue|TypeError|Failed to fetch|terrainFeedbackProbe/,
    );
    expect(text?.length).toBeLessThanOrEqual(100);
    await expect
      .poll(() =>
        consoleDetails.some((value) => value.includes("terrainFeedbackProbe")),
      )
      .toBe(true);
  } finally {
    failure.release();
  }
});

test("terrain warning clears when the failed visible chunk loads without an order", async ({
  page,
}) => {
  const failure = await transientFailure(page);
  try {
    await page.goto("/?play=1");
    await expect.poll(failure.retries, { timeout: 15_000 }).toBe(1);
    // Other visible chunks have succeeded, but the affected one is still held.
    await expect(page.locator("#terrain-cache")).not.toContainText(
      "(0 / 512 chunks)",
    );
    await expect(page.locator("#connection")).not.toHaveText("connected");
    failure.release();
    const canvas = page.locator("#scene");
    await canvas.hover();
    await expect(page.locator("#tile-inspection")).toContainText(
      "geographic height",
    );
    await expect(page.locator("#connection")).toHaveText("connected");
  } finally {
    failure.release();
  }
});

test("hover names a live tree blocker and removes it after depletion", async ({
  page,
}) => {
  const world = await feedbackWorld(page);
  const { canvas, center, target } = await loadedWorld(page);
  const inspection = page.locator("#tile-inspection");
  await canvas.hover({ position: target });
  await expect(inspection).toContainText("tile 8194, 8192:");
  await expect(inspection).toContainText(/tree.*blocked/i);
  await expect(inspection).not.toContainText("passable");
  world.deplete();
  await expect(inspection).toContainText("passable");
  await expect(inspection).not.toContainText("tree");
  // Existing water and open-ground labels remain intact.
  await canvas.hover({ position: { x: center.x + 512, y: center.y } });
  await expect(inspection).toContainText("tile 8196, 8188:");
  await expect(inspection).toContainText("blocked");
  await expect(inspection).not.toContainText("tree");
  await canvas.hover({ position: center });
  await expect(inspection).toContainText("passable");
});

test("a resource-blocked order explains the tree and keeps the unit idle", async ({
  page,
}) => {
  const world = await feedbackWorld(page);
  const { canvas, center, target } = await loadedWorld(page);
  await canvas.click({ position: center });
  await expect(page.locator("#unit-state")).toHaveText("unit idle");
  await canvas.hover({ position: target });
  await expect(page.locator("#tile-inspection")).toContainText(
    "tile 8194, 8192:",
  );
  await canvas.click({ position: target, button: "right" });
  await expect.poll(() => world.orders.length).toBe(1);
  await expect(page.locator("#connection")).toHaveText(
    /cannot move.*tree.*(?:way|block)/i,
  );
  await expect(page.locator("#unit-state")).toHaveText("unit idle");
  await page.getByRole("button", { name: "Center on primary unit" }).click();
  await canvas.hover({ position: center });
  await expect(page.locator("#tile-inspection")).toContainText(
    "tile 8192, 8192:",
  );
});

test("other order rejections use a plain sentence without a tree explanation", async ({
  page,
}) => {
  const world = await feedbackWorld(page);
  const { canvas, center } = await loadedWorld(page);
  await canvas.click({ position: center });
  for (const reason of [1, 2, 3, 4, 5, 6, 7, 8]) {
    world.rejectWith(reason);
    await canvas.click({
      position: { x: center.x + 64, y: center.y + 32 },
      button: "right",
    });
    await expect.poll(() => world.orders.length).toBe(reason);
    await expect(page.locator("#connection")).toHaveText(
      /^[A-Z][^\r\n]{1,119}[.!]$/,
    );
    await expect(page.locator("#connection")).not.toContainText(
      /tree|Rejected[A-Z]|CommandResult|JsValue/,
    );
  }
});

function flatChunk(x: number, y: number, height = 0) {
  const chunk = syntheticChunk(x, y, () => false);
  const bytes = Buffer.from(chunk.payload_hex, "hex");
  for (let tile = 0; tile < 1024; tile++) {
    const offset = 7 + tile * 35;
    bytes.writeInt32LE(height * 100, offset);
    for (let field = 4; field < 14; field += 2)
      bytes.writeInt16LE(height, offset + field);
  }
  return { ...chunk, payload_hex: bytes.toString("hex") };
}

test("elevated failed terrain stays warned until it loads at a stationary camera", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  const world = await feedbackWorld(page, 16384, 8192, 128);
  await page.addInitScript(() => {
    const original = window.fetch.bind(window);
    let failed = false;
    window.fetch = (input, init) => {
      if (!failed && String(input).endsWith("/chunks/258/258")) {
        failed = true;
        return new Promise<Response>((_, reject) => {
          Object.assign(window, {
            failElevatedTerrain: () =>
              reject(new TypeError("elevated terrain probe")),
          });
        });
      }
      return original(input, init);
    };
  });
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route(`**/maps/${hash}/chunks/*/*`, async (route) => {
    const [x = 0, y = 0] = route
      .request()
      .url()
      .split("/")
      .slice(-2)
      .map(Number);
    if (x > 256 || y > 256) await held;
    await route.fulfill({
      json: flatChunk(x, y, x === 258 && y === 258 ? 128 : 0),
    });
  });
  try {
    await page.goto("/?play=1");
    await expect(page.locator("#playground")).toHaveAttribute(
      "data-assets",
      "aoe2-local",
    );
    await page.locator("#scene").hover({ position: { x: 640, y: 360 } });
    await expect(page.locator("#world-position")).toHaveText("8192.0, 8192.0");
    await expect(page.locator("#tile-inspection")).toContainText(
      "tile 8192, 8192:",
    );
    await expect(page.locator("#tile-inspection")).toContainText("height 0");
    await expect
      .poll(() => page.evaluate(() => "failElevatedTerrain" in window))
      .toBe(true);
    const logged = page.waitForEvent("console", {
      predicate: (message) => message.text().includes("elevated terrain probe"),
    });
    await page.evaluate(() =>
      (
        window as unknown as { failElevatedTerrain(): void }
      ).failElevatedTerrain(),
    );
    await logged;
    // Tile (8256,8256), height 128, projects to the centre even though only
    // flat chunks are resident. A stationary frame must not erase its failure.
    await page.evaluate(
      () =>
        new Promise<void>((resolve) =>
          requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
        ),
    );
    await expect(page.locator("#connection")).toHaveText(
      "Some terrain could not be loaded; retrying.",
    );
    expect(world.orders).toEqual([]);
    release();
    await expect(page.locator("#connection")).toHaveText("connected", {
      timeout: 15000,
    });
    await expect(page.locator("#tile-inspection")).toContainText("height 128");
    expect(world.orders).toEqual([]);
  } finally {
    release();
  }
});

test("new terrain failure replaces an old rejection and recovery stays connected", async ({
  page,
}) => {
  const world = await feedbackWorld(page);
  const { canvas, center, target } = await loadedWorld(page);
  await canvas.click({ position: center });
  await canvas.click({ position: target, button: "right" });
  await expect(page.locator("#connection")).toHaveText(
    "Cannot move there: a tree is in the way.",
  );
  let failedUrl = "";
  let retries = 0;
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route(`**/maps/${hash}/chunks/*/*`, async (route) => {
    const url = route.request().url();
    const [x = 0, y = 0] = url.split("/").slice(-2).map(Number);
    if (x >= 350) {
      if (!failedUrl) {
        failedUrl = url;
        await route.abort("failed");
        return;
      }
      if (url === failedUrl) retries++;
      await held;
    }
    await route.fulfill({ json: chunkWithTree(x, y) });
  });
  try {
    await page.getByRole("button", { name: "Show whole map" }).click();
    const box = await page.locator("#minimap-map").boundingBox();
    if (!box) throw new Error("Missing minimap");
    const side = Math.min(box.width, box.height);
    await page.mouse.click(
      box.x + (box.width - side) / 2 + side * 0.75,
      box.y + (box.height - side) / 2 + side * 0.5,
    );
    await page.getByRole("button", { name: "Close map overview" }).click();
    await expect(page.locator("#connection")).toHaveText(
      "Some terrain could not be loaded; retrying.",
    );
    await expect.poll(() => retries, { timeout: 15000 }).toBe(1);
    // The affected chunk is still pending; no newer order or socket reconnect
    // may be needed to replace the rejection or clear the recovered warning.
    await expect(page.locator("#connection")).toHaveText(
      "Some terrain could not be loaded; retrying.",
    );
    expect(world.orders).toHaveLength(1);
    release();
    await expect(page.locator("#connection")).toHaveText("connected");
    expect(world.orders).toHaveLength(1);
  } finally {
    release();
  }
});
import { disabledFog } from "../wire/fog.js";
