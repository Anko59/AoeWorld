import { expect, type Page } from "@playwright/test";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { centerVillage, freshHud } from "../village/fixtures/resources.js";

type Actor = { id: number; kind: string; x: number; y: number; owned: boolean };

/** Private art proof: the real client, normal orders and the arena tick driver. */
export async function townCenterProof(
  page: Page,
  url: string,
  token: string,
  side: "before" | "after",
  agesOnly = false,
) {
  expect(
    token.length,
    "Arena admin tokens require at least 16 characters",
  ).toBeGreaterThanOrEqual(16);
  const directory = `../.cache/tc-shots/client/${side}`;
  await mkdir(directory, { recursive: true });
  const readinessPath = new URL(
    "../../../crates/harness/src/ship/compare/capture/readiness.mjs",
    import.meta.url,
  ).href;
  const readiness = (await import(readinessPath)) as {
    sample(page: Page): Promise<{
      connected: boolean;
      pending: number;
      failed: number;
      frame: number;
      camera: { x: number; y: number; zoom: number } | null;
    }>;
  };
  const response = await page.request.get(
    `${url}asset-pack/buildings/manifest.json`,
  );
  expect(response.ok()).toBe(true);
  const catalog = (await response.json()) as {
    buildings: {
      unit_id: number;
      construction: { available: boolean; angles: number }[];
    }[];
  };
  const bundle = await page.request.get(`${url}pkg/aoe_client_bg.wasm`);
  expect(bundle.ok()).toBe(true);
  const sha256 = (bytes: Buffer) =>
    createHash("sha256").update(bytes).digest("hex");
  const wasmSha256 = sha256(await bundle.body());
  const overlaySha256 = sha256(await response.body());
  expect(wasmSha256).toBe(
    sha256(await readFile("../web/pkg/aoe_client_bg.wasm")),
  );
  const nativeState = () =>
    page.evaluate(async () => {
      const module = (await import(
        new URL("/pkg/aoe_client.js", location.href).href
      )) as {
        client_memory_snapshot(): string;
      };
      return JSON.parse(module.client_memory_snapshot()) as {
        native_failed: number;
        native_request_bytes: number;
        native_resident_frames: number;
      };
    });
  const scene = page.locator("#scene");
  const actors = async (): Promise<Actor[]> =>
    JSON.parse((await scene.getAttribute("data-actors")) ?? "[]") as Actor[];
  const step = async (ticks = 200, orders = 0) => {
    const response = await page.request.post(`${url}match/step`, {
      headers: { "x-aoeworld-admin-token": token },
      data: { ticks, orders },
    });
    expect(response.ok(), await response.text()).toBe(true);
    await freshHud(page);
  };
  const camera = async (x: number, y: number, zoom: number) => {
    await page.evaluate(
      ({ x, y, zoom }) => {
        location.hash = `x=${x}&y=${y}&zoom=${zoom}`;
      },
      { x, y, zoom },
    );
    await expect(page.locator("#minimap")).toHaveAttribute(
      "data-zoom",
      String(zoom),
    );
    await expect(page.locator("#minimap")).toHaveAttribute(
      "data-camera-x",
      String(x),
    );
    await expect(page.locator("#minimap")).toHaveAttribute(
      "data-camera-y",
      String(y),
    );
    await expect(scene).toHaveAttribute("data-chunks-pending", "0");
    await freshHud(page);
  };
  const select = async (kind: string, id?: number) => {
    const actor = (await actors()).find(
      (actor) =>
        actor.kind === kind &&
        actor.owned &&
        (id === undefined || actor.id === id),
    );
    if (!actor) throw new Error(`${kind} ${id ?? ""} missing`);
    await page.mouse.click(actor.x, actor.y);
    await expect(page.locator("#selection")).toHaveAttribute("data-kind", kind);
  };
  const capture = async (name: string) => {
    await expect(scene).toHaveAttribute("data-village-art", "native");
    await expect(scene).toHaveAttribute("data-chunks-pending", "0");
    await expect(scene).toHaveAttribute("data-chunks-failed", "0");
    const initial = await readiness.sample(page);
    if (!initial.camera) throw new Error("Capture camera missing");
    const { x, y, zoom } = initial.camera;
    // Explicit camera changes require new complete presentations, including in
    // static scenes. Ambient animation and bounded lookahead may keep running.
    await camera(x + 0.125, y, zoom);
    await expect
      .poll(async () => Number(await scene.getAttribute("data-frame")))
      .toBeGreaterThan(initial.frame);
    const displaced = await readiness.sample(page);
    await camera(x, y, zoom);
    await expect
      .poll(async () => Number(await scene.getAttribute("data-frame")))
      .toBeGreaterThan(displaced.frame);
    const presentation = await readiness.sample(page);
    expect(presentation.connected).toBe(true);
    expect(presentation.pending).toBe(0);
    expect(presentation.failed).toBe(0);
    const native = await nativeState();
    expect(native.native_failed).toBe(0);
    expect(native.native_request_bytes).toBeLessThanOrEqual(64 * 1024 ** 2);
    expect(native.native_resident_frames).toBeGreaterThan(0);
    await page.screenshot({ path: `${directory}/${name}.png` });
    // Intentional temporal sample, not a readiness delay: inspect the same
    // building again after animation has had time to choose another cell.
    await page.waitForTimeout(1000);
    await camera(x + 0.125, y, zoom);
    await expect
      .poll(async () => Number(await scene.getAttribute("data-frame")))
      .toBeGreaterThan(presentation.frame);
    const secondDisplaced = await readiness.sample(page);
    await camera(x, y, zoom);
    await expect
      .poll(async () => Number(await scene.getAttribute("data-frame")))
      .toBeGreaterThan(secondDisplaced.frame);
    await page.screenshot({ path: `${directory}/${name}-frame-2.png` });
    await writeFile(
      `${directory}/${name}.json`,
      JSON.stringify(
        {
          health: await (await page.request.get(`${url}health`)).json(),
          presentation,
          native,
          wasmSha256,
          overlaySha256,
          secondPresentation: await readiness.sample(page),
          age: await page.locator("#age").textContent(),
          selection: await page.locator("#selection").innerText(),
          progress: await page
            .locator("#construction-progress")
            .getAttribute("value"),
          actors: await actors(),
          camera: await page
            .locator("#minimap")
            .evaluate((map) =>
              ["data-camera-x", "data-camera-y", "data-zoom"].map((key) =>
                map.getAttribute(key),
              ),
            ),
        },
        null,
        2,
      ),
    );
  };
  const open = async (scenario: string) => {
    expect(
      (
        await page.request.post(`${url}maps/reset?scenario=${scenario}`)
      ).status(),
    ).toBe(204);
    await page.goto(`${url}?play=1`);
    await expect(page.locator("#connection")).toHaveText("connected", {
      timeout: 60_000,
    });
    await centerVillage(page, true);
    const at = await page.locator("#minimap").evaluate((map) => ({
      x: Number(map.getAttribute("data-camera-x")),
      y: Number(map.getAttribute("data-camera-y")),
    }));
    // Leave room above the taller Feudal/Castle roof and below the HUD.
    await camera(at.x - 1, at.y - 1, 0.85);
    return at;
  };
  await open("feudal-economy");
  await expect(page.locator("#age")).toContainText("Dark Age");
  await capture("dark-complete");
  const home = await open("feudal-military");
  await expect(page.locator("#age")).toContainText("Feudal Age");
  await capture("feudal-complete");
  await page.locator("#research-castle-age").click();
  await step(1, 1);
  for (
    let i = 0;
    i < 12 &&
    !(await page.locator("#age").textContent())?.includes("Castle Age");
    i++
  )
    await step();
  await expect(page.locator("#age")).toContainText("Castle Age");
  await capture("castle-complete");
  if (agesOnly) return;

  const build = async (
    kind: string,
    command: string,
    military: boolean,
    unitId: number,
  ) => {
    const recipe = catalog.buildings.find((b) => b.unit_id === unitId);
    if (!recipe) throw new Error(`DAT recipe ${unitId} missing`);
    const stages = Math.max(
      ...recipe.construction.filter((p) => p.available).map((p) => p.angles),
    );
    expect(stages).toBeGreaterThan(0);
    await camera(home.x, home.y, 0.6);
    await select("Villager");
    const previous = new Set(
      (await actors()).filter((a) => a.kind === kind).map((a) => a.id),
    );
    await page
      .locator(military ? "#build-military-menu" : "#build-menu")
      .click();
    await page.locator(`#build-${command}`).click();
    let placed: { x: number; y: number } | undefined;
    for (const [dx, dy] of [
      [12, 2],
      [-12, -6],
      [4, 12],
      [-8, 12],
      [12, -10],
      [-16, 2],
    ] as const) {
      const at = { x: home.x + dx, y: home.y + dy };
      await camera(at.x, at.y, 0.6);
      const bounds = await scene.boundingBox();
      if (!bounds) throw new Error("Scene missing");
      await page.mouse.move(
        bounds.x + bounds.width / 2,
        bounds.y + bounds.height / 2,
      );
      await freshHud(page);
      if ((await scene.getAttribute("data-placement-valid")) !== "true")
        continue;
      await page.mouse.click(
        bounds.x + bounds.width / 2,
        bounds.y + bounds.height / 2,
      );
      placed = at;
      break;
    }
    if (!placed) throw new Error(`No legal ${kind} site`);
    await step(1, 1);
    const actor = (await actors()).find(
      (a) => a.kind === kind && !previous.has(a.id),
    );
    if (!actor) throw new Error(`Placed ${kind} missing`);
    const side = kind === "StoneGate" || kind === "PalisadeGate" ? 3 : 4;
    await camera(
      placed.x + side / 2,
      placed.y + side / 2,
      kind === "Castle" ? 0.75 : 1.1,
    );
    await select(kind, actor.id);
    await capture(`${command}-stage-0`);
    const captured = new Set([0]);
    for (let i = 0; i < 300; i++) {
      await step(10);
      const progress = Number(
        await page.locator("#construction-progress").getAttribute("value"),
      );
      if (progress >= 1000) break;
      const stage = Math.floor((progress * stages) / 1000);
      if (!captured.has(stage)) {
        await capture(`${command}-stage-${stage}`);
        captured.add(stage);
      }
    }
    await expect(page.locator("#building-status")).toHaveText("Complete");
    expect(
      [...captured].sort(),
      `${kind} must show every DAT construction angle`,
    ).toEqual(Array.from({ length: stages }, (_, i) => i));
    await capture(`${command}-complete`);
  };
  await build("TownCenter", "town-center", false, 141);
  await build("Castle", "castle", true, 82);
  await build("StoneGate", "stone-gate", true, 64);
  await build("PalisadeGate", "palisade-gate", true, 792);
}
