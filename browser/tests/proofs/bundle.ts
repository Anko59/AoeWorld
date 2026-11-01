import { expect, type Page } from "@playwright/test";
import { writeFile } from "node:fs/promises";
import { syntheticChunk } from "../synthetic-map.js";
import { disabledFog } from "../wire/fog.js";
import { marketPrices } from "../wire/buildings.js";
import { freshHud } from "../village/fixtures/resources.js";
import { position, sint, uint } from "../village/input/fixture.js";

/** Frozen presentation proof, served only by this checkout's private dev server.
 * Native artwork and real shaders/input render a combined current-wire scene.
 * Simulation behavior is covered separately; this is not a running RTS match.
 */
export async function bundleProof(page: Page, url: string, film = false) {
  expect((await page.request.get(`${url}asset-pack/manifest.json`)).ok()).toBe(
    true,
  );
  const unit = (
    id: number,
    kind: number,
    x: number,
    y: number,
    hp: number,
    cargo = 0,
  ) => [
    ...uint(id),
    0,
    ...position(x, y),
    0,
    0,
    0,
    kind,
    0,
    0,
    0,
    ...uint(hp),
    ...uint(hp),
    0,
    0,
    0,
    0,
    0,
    0,
    cargo,
    0, // Combined wire: no Vassal home Castle.
    0, // Combined wire: no Vassal manager.
    0, // Aggressive stance.
    0, // No General commander.
    0, // No General summary.
  ];
  const building = (
    id: number,
    kind: number,
    x: number,
    y: number,
    w: number,
    h: number,
    hp: number,
    garrison: number[][] = [],
  ) => [
    id,
    0,
    kind,
    ...sint(x),
    ...sint(y),
    ...sint(x + w),
    ...sint(y + h),
    1,
    ...uint(1000),
    ...uint(hp),
    ...uint(hp),
    8,
    ...(kind === 5 ? [1, ...uint(375)] : [0]),
    0,
    0,
    0,
    2,
    0,
    0,
    0,
    garrison.length,
    ...garrison.flat(),
    ...(kind === 20
      ? [1, ...sint(x), ...sint(y), ...sint(x + w - 1), ...sint(y + h - 1)]
      : [0]),
  ];
  const units = [
    unit(0, 1, 8192.5, 8192.5, 45),
    unit(2, 0, 8188.5, 8189.5, 40),
    unit(3, 0, 8187.5, 8195.5, 40),
    unit(4, 0, 8188.5, 8195.5, 40),
    ...[0, 1, 2, 3].map((i) => unit(10 + i, 12, 8190.5 + i, 8185.5, 100)),
    unit(15, 14, 8191.5, 8194.5, 30),
    unit(16, 8, 8192.5, 8194.5, 60),
    unit(17, 20, 8197.5, 8194.5, 100, 2),
  ];
  const buildings = [
    building(30, 0, 8187, 8190, 4, 4, 2400, [
      [100, 0, 40, 40, 0],
      [101, 0, 40, 40, 0],
    ]),
    building(31, 1, 8186, 8187, 2, 2, 900),
    building(32, 4, 8186, 8196, 2, 2, 1000),
    building(33, 5, 8189, 8196, 3, 3, 480),
    building(34, 11, 8191, 8187, 3, 3, 1800),
    building(35, 16, 8199, 8187, 4, 4, 4800),
    building(36, 20, 8195, 8191, 5, 1, 1200),
  ];
  await page.routeWebSocket("**/game/ws", (socket) => {
    socket.onMessage((message) => {
      const bytes =
        typeof message === "string" ? Buffer.from(message) : message;
      if (bytes[0] === 0)
        socket.send(
          Buffer.from([
            0,
            4,
            1,
            0,
            0,
            ...uint(32768),
            ...uint(32768),
            ...uint(1024),
            20,
            0,
            ...Array<number>(24).fill(1),
          ]),
        );
      if (bytes[0] !== 1) return;
      const revision: number[] = [];
      for (const byte of bytes.subarray(1)) {
        revision.push(byte);
        if (byte < 128) break;
      }
      socket.send(
        Buffer.from([
          1,
          ...revision,
          0,
          units.length,
          ...units.flat(),
          1,
          0,
          ...uint(2000),
          ...uint(2000),
          ...uint(2000),
          ...uint(2000),
          15,
          2,
          ...uint(4 + 16384),
          ...marketPrices,
          buildings.length,
          ...buildings.flat(),
        ]),
      );
      socket.send(disabledFog(revision));
    });
  });
  await page.goto(`${url}?play=1#x=8193&y=8192&zoom=.55&renderer=webgl2`);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    "webgl2",
  );
  await expect(page.locator("#age")).toContainText("Castle Age");
  const chunks = [-1, 0].flatMap((y) =>
    [-1, 0].map((x) => {
      const chunk = syntheticChunk(256 + x, 256 + y, false);
      const bytes = Buffer.from(chunk.payload_hex, "hex");
      for (let i = 0; i < 1024; i++) {
        const tx = chunk.x * 32 + (i % 32);
        if (tx >= 8196 && tx <= 8198)
          bytes.writeUInt32LE(11 | (3 << 12), 7 + i * 35 + 14);
      }
      return { ...chunk, payload_hex: bytes.toString("hex") };
    }),
  );
  await page.evaluate(async (payload) => {
    const game = await import(
      new URL("/pkg/aoe_client.js", location.href).href
    );
    game.activate_surface_fixture("05".repeat(32), payload);
  }, JSON.stringify(chunks));
  if (film) {
    await page.evaluate(() => {
      const label = document.createElement("div");
      label.textContent =
        "Frozen presentation fixture · original local art · Firefox WebGL2";
      label.style.cssText =
        "position:fixed;top:58px;left:12px;background:#000b;color:white;padding:8px;z-index:9999;pointer-events:none";
      document.body.append(label);
    });
  }
  const actors = async () =>
    JSON.parse(
      (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
    ) as { id: number; kind: string; x: number; y: number }[];
  const select = async (kind: string, double = false) => {
    const actor = (await actors()).find((a) => a.kind === kind);
    if (!actor) throw new Error(`${kind} absent from combined proof`);
    if (double) await page.mouse.dblclick(actor.x, actor.y);
    else await page.mouse.click(actor.x, actor.y);
    await expect(page.locator("#selection-name")).toContainText(
      kind === "TownCenter"
        ? "Town Center"
        : kind === "TransportShip"
          ? "Transport Ship"
          : kind,
    );
  };
  const capture = async (name: string) => {
    const path = `../.cache/ui/bundle-${name}-firefox`;
    await freshHud(page);
    await page.screenshot({ path: `${path}.png` });
    if (film) await page.waitForTimeout(5000);
    await writeFile(
      `${path}.json`,
      JSON.stringify(
        {
          url,
          fixture: "frozen combined Castle presentation",
          water: { kind: "River", x: [8196, 8198] },
          actors: await actors(),
        },
        null,
        2,
      ),
    );
  };
  await expect
    .poll(async () => (await actors()).some((a) => a.kind === "TransportShip"))
    .toBe(true);
  await select("Knight", true);
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "4");
  await page.locator("#formation-line").click();
  await expect(page.locator("#formation-line")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await capture("formation-village");
  await select("TownCenter");
  await expect(page.locator("#garrison-count")).toHaveText("2 / 15");
  await capture("garrison-bridge");
  await select("TransportShip");
  await expect(page.locator("#selection-load")).toHaveText("2/5 passengers");
  await capture("ship-on-river");
}
