import { expect } from "@playwright/test";
import { PNG } from "pngjs";
import { test } from "../../../server.js";
import { centerVillage, freshHud } from "../fixtures/resources.js";
import { gameAssets } from "../../game-assets.js";
import {
  box,
  frameSize,
  nativeMenu,
  panels,
  S,
  zoomFor,
} from "../fixtures/native-menu.js";
for (const [width, height] of [
  [1280, 720],
  [1920, 1080],
  [1100, 700],
  [900, 600],
  [3840, 2160],
] as const) {
  test(`bottom menu art covers every panel at ${width}x${height}`, async ({
    page,
  }) => {
    test.setTimeout(90_000);
    await page.setViewportSize({ width, height });
    await nativeMenu(page);
    const zoom = zoomFor(width);
    const tolerance = 1.2;
    const commands = await box(page, "#commands");
    const selection = await box(page, "#selection");
    const minimap = await box(page, "#minimap");
    // Fixed panels are exactly the size of their frame art.
    for (const [role, rect] of [
      ["commands", commands],
      ["minimap", minimap],
    ] as const) {
      expect(Math.abs(rect.width - frameSize(role).width * zoom)).toBeLessThan(
        tolerance,
      );
      expect(
        Math.abs(rect.height - frameSize(role).height * zoom),
      ).toBeLessThan(tolerance);
    }
    expect(
      Math.abs(selection.height - frameSize("selection").height * zoom),
    ).toBeLessThan(tolerance);
    expect(selection.width).toBeGreaterThanOrEqual(
      (322 + 700) * S * zoom - tolerance,
    );
    // Cover check below samples inside each box, so overlaps must not hide the corners.
    // The three panels span the whole window with no empty strip: each overlaps its
    // neighbour by the transparent overhang (sword tips) of the art, never leaving a gap.
    expect(Math.abs(commands.x)).toBeLessThan(tolerance);
    expect(
      Math.abs(selection.x - (commands.x + commands.width - 5.5 * zoom)),
    ).toBeLessThan(tolerance);
    expect(
      Math.abs(minimap.x - (selection.x + selection.width - 31 * zoom)),
    ).toBeLessThan(tolerance);
    expect(Math.abs(minimap.x + minimap.width - width)).toBeLessThan(tolerance);
    for (const rect of [commands, selection, minimap]) {
      expect(Math.abs(rect.y + rect.height - height)).toBeLessThan(tolerance);
    }
    // Every art layer lies inside its panel and the layers tile its full width.
    for (const selector of ["#commands", "#selection", "#minimap"]) {
      const parent = await box(page, selector);
      const layers = await page
        .locator(`${selector}>.frame-art`)
        .evaluateAll((nodes) =>
          nodes.map((node) => {
            const r = node.getBoundingClientRect();
            return { x: r.x, y: r.y, width: r.width, height: r.height };
          }),
        );
      expect(layers.length).toBeGreaterThan(0);
      let covered = 0;
      for (const layer of layers) {
        expect(Math.abs(layer.y - parent.y)).toBeLessThan(tolerance);
        expect(Math.abs(layer.height - parent.height)).toBeLessThan(tolerance);
        expect(layer.x).toBeGreaterThanOrEqual(parent.x - tolerance);
        expect(layer.x + layer.width).toBeLessThanOrEqual(
          parent.x + parent.width + tolerance,
        );
        covered += layer.width;
      }
      expect(Math.abs(covered - parent.width)).toBeLessThan(tolerance);
    }
    // Rendered pixels agree: sampled points just inside each panel are art, not scene.
    const shot = PNG.sync.read(await page.screenshot());
    const dpr = shot.width / width;
    const sample = (x: number, y: number) => {
      const offset =
        (Math.round(y * dpr) * shot.width + Math.round(x * dpr)) * 4;
      return [...shot.data.slice(offset, offset + 3)];
    };
    const near = (actual: number[], hex: string) => {
      const want = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
      return actual.every((v, i) => Math.abs(v - (want[i] ?? 0)) < 12);
    };
    for (const [role, rect] of [
      ["commands", commands],
      ["selection", selection],
      ["minimap", minimap],
    ] as const) {
      const right =
        rect.width -
        3 -
        (role === "commands" ? 5.5 : role === "selection" ? 31 : 0) * zoom;
      for (const [fx, fy] of [
        [2, 2],
        [right, 2],
        [2, rect.height - 3],
        [right, rect.height - 3],
      ] as const) {
        // The transparent corners of the art-less rows do not exist: the frame is cut to its bounds.
        expect(
          near(sample(rect.x + fx, rect.y + fy), panels[role].colour),
        ).toBe(true);
      }
    }
    // The minimap map fills the rhombus rectangle of its frame, clipped to a diamond.
    const map = await box(page, "#minimap-map");
    const expected = {
      x: 53.5 * zoom,
      y: 8.5 * zoom,
      width: 339.5 * zoom,
      height: 184 * zoom,
    };
    expect(Math.abs(map.x - (minimap.x + expected.x))).toBeLessThan(tolerance);
    expect(Math.abs(map.y - (minimap.y + expected.y))).toBeLessThan(tolerance);
    expect(Math.abs(map.width - expected.width)).toBeLessThan(tolerance);
    expect(Math.abs(map.height - expected.height)).toBeLessThan(tolerance);
    // Buttons sit in the four sockets (corners) and the small ones inside the rhombus.
    for (const id of [
      "town-center",
      "recenter",
      "overview-toggle",
      "grid",
      "carry-indicators",
      "fullscreen",
      "minimap-zoom",
    ]) {
      const rect = await box(page, `#${id}`);
      expect(rect.x).toBeGreaterThanOrEqual(minimap.x - tolerance);
      expect(rect.y).toBeGreaterThanOrEqual(minimap.y - tolerance);
      expect(rect.x + rect.width).toBeLessThanOrEqual(
        minimap.x + minimap.width + tolerance,
      );
      expect(rect.y + rect.height).toBeLessThanOrEqual(
        minimap.y + minimap.height + tolerance,
      );
    }
    const socket = await box(page, "#town-center");
    expect(
      Math.abs(socket.x + socket.width / 2 - (minimap.x + 76 * zoom)),
    ).toBeLessThan(2 * zoom + tolerance);
    expect(
      Math.abs(socket.y + socket.height / 2 - (minimap.y + 58.5 * zoom)),
    ).toBeLessThan(2 * zoom + tolerance);
    await expect(page.locator("#minimap")).not.toHaveClass(/expanded/);
    await page.screenshot({
      path: `../.cache/bottom/synthetic-${width}x${height}.png`,
    });
  });
}

test("the minimap map keeps filling the diamond rectangle at far zoom", async ({
  page,
}) => {
  test.setTimeout(90_000);
  await page.setViewportSize({ width: 1920, height: 1080 });
  await nativeMenu(page);
  const close = await box(page, "#minimap-map");
  await page.evaluate(() => {
    location.hash = "x=8192.5&y=8192.5&zoom=.04";
  });
  await expect(page.locator("html")).toHaveAttribute(
    "data-lod",
    /^(map|markers)$/,
  );
  // The first far view plays the cockpit reveal, which slides #minimap up 12px
  // over 0.8s (reveal.css). It starts asynchronously after data-lod flips (the
  // identity digest is awaited first), so wait for it to be recorded as shown
  // and then finished; measuring mid-slide moved the map 2px off the frame.
  await page.waitForFunction(
    () => sessionStorage.getItem("aoeworld.cockpit-reveal") !== null,
  );
  await expect(page.locator("html")).not.toHaveAttribute(
    "data-cockpit-reveal",
    /.+/,
    { timeout: 15_000 },
  );
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          document.getElementById("minimap")?.getAnimations({ subtree: true })
            .length,
      ),
    )
    .toBe(0);
  const minimap = await box(page, "#minimap");
  const map = await box(page, "#minimap-map");
  const view = await box(page, "#minimap-view");
  const expected = { x: 53.5, y: 8.5, width: 339.5, height: 184 };
  expect(Math.abs(map.x - (minimap.x + expected.x))).toBeLessThan(1.2);
  expect(Math.abs(map.y - (minimap.y + expected.y))).toBeLessThan(1.2);
  expect(Math.abs(map.width - expected.width)).toBeLessThan(1.2);
  expect(Math.abs(map.height - expected.height)).toBeLessThan(1.2);
  // The far-zoom view layer covers the whole map rectangle, not a patch of it.
  for (const key of ["x", "y", "width", "height"] as const)
    expect(Math.abs(view[key] - map[key])).toBeLessThan(1.2);
  for (const key of ["width", "height"] as const)
    expect(Math.abs(map[key] - close[key])).toBeLessThan(1.2);
});

test("the whole-map overlay expands over the native menu and closes back to the diamond", async ({
  page,
}) => {
  test.setTimeout(90_000);
  await page.setViewportSize({ width: 1280, height: 720 });
  await nativeMenu(page);
  const small = await box(page, "#minimap");
  const diamond = await box(page, "#minimap-map");
  for (const [index, open] of [
    () => page.keyboard.press("m"),
    () => page.locator("#overview-toggle").click(),
  ].entries()) {
    await open();
    await expect(page.locator("#minimap")).toHaveClass(/expanded/);
    await expect(page.locator("#minimap")).toHaveCSS("position", "fixed");
    // game.css: inset 72px 25px 22px of the window.
    const big = await box(page, "#minimap");
    if (
      Math.abs(big.x - 25) +
        Math.abs(big.y - 72) +
        Math.abs(big.width - 1230) +
        Math.abs(big.height - 626) >=
      3
    )
      throw new Error(`big ${JSON.stringify(big)}`);
    const map = await box(page, "#minimap-map");
    expect(map.width).toBeGreaterThan(big.width * 0.8);
    expect(map.height).toBeGreaterThan(big.height * 0.7);
    await expect(page.locator("#minimap-close")).toBeVisible();
    await expect(page.locator("#minimap>.frame-art:visible")).toHaveCount(0);
    // Click-to-camera still maps the overlay: opposite sides of its centre
    // give symmetric world points around the centre click.
    const svg = await box(page, "#minimap-map");
    const camera = () =>
      page
        .locator("#minimap")
        .evaluate((el) => [
          Number(el.dataset.cameraX),
          Number(el.dataset.cameraY),
        ]);
    const jump = async (dx: number) => {
      const before = await camera();
      await page.mouse.click(
        svg.x + svg.width / 2 + dx,
        svg.y + svg.height / 2,
      );
      await expect.poll(camera).not.toEqual(before);
      return camera();
    };
    if (index === 0) {
      const side = Math.min(svg.width, svg.height) / 4;
      const [lx = 0, ly = 0] = await jump(-side);
      const [rx = 0, ry = 0] = await jump(side);
      expect(rx - lx).toBeGreaterThan(100);
      // The map is isometric: a horizontal step moves x and y in opposite directions.
      expect(ly - ry).toBeCloseTo(rx - lx, -1);
      expect(Math.abs(lx + rx - (ly + ry))).toBeLessThan(2);
    }
    await page.keyboard.press("Escape");
    await expect(page.locator("#minimap")).not.toHaveClass(/expanded/);
    const closed = await box(page, "#minimap");
    if (
      Math.abs(closed.x - small.x) +
        Math.abs(closed.width - small.width) +
        Math.abs(closed.height - small.height) >=
      1.5
    )
      throw new Error(`closed ${JSON.stringify([closed, small])}`);
    const again = await box(page, "#minimap-map");
    if (
      Math.abs(again.width - diamond.width) +
        Math.abs(again.height - diamond.height) >=
      1.5
    )
      throw new Error(`diamond ${JSON.stringify([again, diamond])}`);
    await expect(page.locator("#minimap>.frame-art:visible")).not.toHaveCount(
      0,
    );
    await expect(page.locator("#minimap-close")).toBeHidden();
  }
});

test("an enemy portrait uses the colour its sprites are rendered with", async ({
  page,
}) => {
  test.setTimeout(90_000);
  // Player 0 chooses red: the renderer swaps red and blue, so player 1 is blue.
  await page.addInitScript(() =>
    sessionStorage.setItem("aoeworld.player-colour", "red"),
  );
  await page.setViewportSize({ width: 1280, height: 900 });
  await nativeMenu(page, true);
  await freshHud(page);
  const actor = await page.locator("#scene").evaluate((el) =>
    (
      JSON.parse(el.getAttribute("data-actors") ?? "[]") as {
        id: number;
        x: number;
        y: number;
      }[]
    ).find((a) => a.id === 1),
  );
  if (!actor) throw new Error("No foreign villager");
  await page.mouse.click(actor.x, actor.y - 10);
  await expect(page.locator("#selection-owner")).toHaveText("Player 2");
  await expect(page.locator("#selection-portrait .art-icon")).toHaveAttribute(
    "style",
    /--tint:rgb\(65,145,245\)/,
  );
});

// Evidence with the owner's local pack: skipped on public CI where it is absent.
for (const [width, height] of [
  [1280, 720],
  [1920, 1080],
  [1100, 700],
] as const) {
  test(`real DE bottom menu at ${width}x${height}`, async ({
    page,
    request,
  }) => {
    test.setTimeout(120_000);
    test.skip(
      !(await request.get("/asset-pack/buildings/interface.json")).ok(),
      "No local DE pack",
    );
    await page.setViewportSize({ width, height });
    await gameAssets(page);
    await page.goto("/?play=1");
    await expect(page.locator("#connection")).toHaveText("connected");
    await expect(page.locator("#bottom-panel")).toHaveAttribute(
      "data-frame",
      "native",
    );
    const shot = (name: string) =>
      page
        .locator("#bottom-panel")
        .screenshot({ path: `../.cache/bottom/real-${name}-${width}.png` });
    await shot("empty");
    await centerVillage(page, true);
    await expect(page.locator("#selection-name")).toHaveText("Town Center");
    await shot("towncenter");
    const villager = await page.locator("#scene").evaluate((el) =>
      (
        JSON.parse(el.getAttribute("data-actors") ?? "[]") as {
          kind: string;
          owned: boolean;
          x: number;
          y: number;
        }[]
      ).find((a) => a.kind === "Villager" && a.owned),
    );
    if (!villager) throw new Error("No villager");
    for (const [name, colour] of [
      ["blue", "#4191f5"],
      ["red", "#f54141"],
      ["yellow", "#f5dc28"],
    ] as const) {
      await page.evaluate(
        (c) => document.documentElement.style.setProperty("--player-colour", c),
        colour,
      );
      await page.keyboard.press("Escape");
      await page.mouse.click(villager.x, villager.y);
      await expect(page.locator("#selection-name")).toHaveText("Villager");
      expect(
        await page
          .locator("#selection-portrait .art-icon")
          .getAttribute("style"),
      ).toContain(colour);
      await shot(`villager-${name}`);
    }
    await page.mouse.dblclick(villager.x, villager.y);
    const tokens = page.locator("#selected-icons .unit-token");
    await expect.poll(() => tokens.count()).toBeGreaterThan(1);
    const before = await tokens.count();
    await shot("group");
    // Shift-click removes a unit from the selection; a plain click selects just it.
    await tokens.first().click({ modifiers: ["Shift"] });
    await expect.poll(() => tokens.count()).toBe(before - 1);
    await tokens.first().click();
    await expect(page.locator("#selection-name")).toHaveText("Villager");
    await expect(page.locator("#selected-icons .unit-token")).toHaveCount(0);
    await expect(page.locator("#stop-units")).toBeVisible();
    await page.screenshot({ path: `../.cache/bottom/real-full-${width}.png` });
  });
}
