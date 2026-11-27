import { expect, type Page } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { ownersFixture } from "../rendering/fixtures/owners.js";
import { nativeMenu } from "../village/fixtures/native-menu.js";
import { fiveVillagerFixture } from "../village/fixtures/village.js";
import { spilled, stillMap, type Still } from "./still-map.js";

/** The marker's painted box and its distance from the camera, in page px. */
async function marker(page: Page) {
  return page.evaluate(async () => {
    const { minimapProject } = await import("/minimap.js" as string);
    const panel = document.getElementById("minimap");
    const map = document.getElementById("minimap-map");
    const root = document.getElementById("minimap-marker");
    if (!panel || !map || !root) throw new Error("#minimap-marker is missing");
    const boxes = [root, ...root.querySelectorAll("*")]
      .filter(
        (node): node is SVGGeometryElement =>
          node instanceof SVGGeometryElement &&
          !node.closest("defs,clipPath,mask,symbol") &&
          node.checkVisibility({
            checkOpacity: true,
            checkVisibilityCSS: true,
          }) &&
          node.getBBox().width + node.getBBox().height > 0,
      )
      .map((node) => node.getBoundingClientRect());
    const left = Math.min(...boxes.map((box) => box.left));
    const right = Math.max(...boxes.map((box) => box.right));
    const top = Math.min(...boxes.map((box) => box.top));
    const bottom = Math.max(...boxes.map((box) => box.bottom));
    const projected: number[] = minimapProject(
      [Number(panel.dataset["cameraX"]), Number(panel.dataset["cameraY"])],
      [
        Number(panel.dataset["worldWidth"]),
        Number(panel.dataset["worldHeight"]),
      ],
    );
    const rect = map.getBoundingClientRect();
    const side = Math.min(rect.width, rect.height);
    const stretched = map.getAttribute("preserveAspectRatio") === "none";
    const width = stretched ? rect.width : side;
    const height = stretched ? rect.height : side;
    const x =
      rect.left +
      (rect.width - width) / 2 +
      ((projected[0] ?? NaN) / 256) * width;
    const y =
      rect.top +
      (rect.height - height) / 2 +
      ((projected[1] ?? NaN) / 256) * height;
    return {
      pulse: root.getAttribute("data-pulse"),
      width: right - left,
      height: bottom - top,
      apart: Math.hypot((left + right) / 2 - x, (top + bottom) / 2 - y),
      map: [rect.width, rect.height] as const,
    };
  });
}

/** Waits for a settled marker on a map of about that width. */
async function located(page: Page, mapWidth: number, height: number) {
  await expect(async () => {
    const seen = await marker(page);
    expect(Math.abs(seen.map[0] - mapWidth)).toBeLessThan(2);
    expect(seen.pulse).toBeNull();
    expect(seen.width).toBeGreaterThanOrEqual(24);
    expect(seen.height).toBeGreaterThanOrEqual(16);
    expect(seen.height).toBeLessThan(height + 12);
    expect(seen.apart).toBeLessThanOrEqual(1.5);
  }).toPass({ timeout: 15_000 });
}

test("the live minimap marks the camera at unit zoom in every menu size", async ({
  page,
}) => {
  test.setTimeout(90_000);
  await page.setViewportSize({ width: 1280, height: 720 });
  await nativeMenu(page);
  await page.evaluate(() => {
    location.hash = "x=5000&y=9500&zoom=1";
  });
  const minimap = page.locator("#minimap");
  await expect(minimap).toHaveAttribute("data-camera-x", "5000");
  await expect(minimap).toHaveAttribute("data-minimap-zoom", "1");
  await located(page, 339.5, 20);
  // Narrow windows scale the whole menu with CSS zoom (bottom-menu.css).
  await page.setViewportSize({ width: 1100, height: 720 });
  await located(page, 339.5 * 0.8, 20);
  await page.setViewportSize({ width: 760, height: 720 });
  await located(page, 339.5 * 0.5, 20);
  // The overview letterboxes the same map and gets the larger locator.
  await page.setViewportSize({ width: 1280, height: 720 });
  await located(page, 339.5, 20);
  await page.keyboard.press("m");
  await expect(minimap).toHaveClass(/expanded/);
  await expect
    .poll(async () => (await marker(page)).map[1])
    .toBeGreaterThan(400);
  const wide = (await marker(page)).map[0];
  await located(page, wide, 32);
  await expect(minimap).toHaveAttribute("data-camera-x", "5000");
});

for (const frame of ["native", "square", "overview"] as const)
  test(`a large view draws nothing beyond the ${frame} map`, async ({
    page,
  }) => {
    test.setTimeout(60_000);
    await stillMap(page, frame);
    for (const [world, zoom, minimap] of [
      [16384, 0.02, 90],
      [16384, 0.25, 1024],
      [2000, 1, 1024],
    ] as const) {
      const look = await page.evaluate(
        async ([world, zoom, minimap]) => {
          const still = Reflect.get(window, "still") as Still;
          still.wheel(1 / 4096);
          still.wheel(minimap);
          still.present(world * 0.53, world * 0.47, zoom, world);
          await still.frames(2);
          return still.drawn();
        },
        [world, zoom, minimap] as const,
      );
      expect(look.kind).toBe("cover");
      expect(look.filled, "a cover has no lens").toBe(0);
      expect(look.open, "a cover has no brackets").toBe(0);
      const seen = await spilled(page);
      expect(seen.outside).toBe(0);
      expect(seen.inside).toBeGreaterThan(100);
    }

    // A frame dragged half off the zoomed minimap.
    const map = await page.evaluate(async () => {
      const still = Reflect.get(window, "still") as Still;
      still.wheel(1 / 4096);
      still.wheel(20);
      still.present(8192, 8192, 0.02, 16384);
      await still.frames(2);
      return still.map;
    });
    const x = map.left + map.width / 2;
    const y = map.top + map.height / 2;
    await page.mouse.move(x, y);
    await page.mouse.down();
    await page.mouse.move(x - map.width * 0.45, y, { steps: 6 });
    await page.mouse.up();
    const kind = await page.evaluate(async () => {
      const still = Reflect.get(window, "still") as Still;
      still.present(8192, 8192, 0.02, 16384);
      await still.frames(2);
      return still.drawn().kind;
    });
    expect(kind).toBe("frame");
    const dragged = await spilled(page);
    expect(dragged.outside).toBe(0);
    expect(dragged.inside).toBeGreaterThan(100);

    // A jump between two places near the border, held at its widest pulse.
    await page.evaluate(async () => {
      const still = Reflect.get(window, "still") as Still;
      still.wheel(1 / 4096);
      still.present(...still.at(-0.175 * still.map.width, 0), 1 / 60);
      await still.frames(2);
      still.present(...still.at(0.175 * still.map.width, 0), 1 / 60);
      for (const animation of document.getAnimations()) {
        animation.pause();
        animation.currentTime = 0;
      }
    });
    expect((await spilled(page)).outside).toBe(0);
  });

test("a letterboxed map marks the camera wherever its land is visible", async ({
  page,
}) => {
  await stillMap(page, "overview");
  const { map, box } = await page.evaluate(() => {
    const still = Reflect.get(window, "still") as Still;
    const box = document.getElementById("minimap-map")?.getBoundingClientRect();
    if (!box) throw new Error("#minimap-map is missing");
    still.wheel(8);
    still.present(1000, 1000);
    return { map: still.map, box: { left: box.left, width: box.width } };
  });
  const x = Math.round(map.left + map.width / 2);
  const y = Math.round(map.top + map.height / 2);
  // The bars beside the square are wide enough for the place of the camera.
  expect(map.width / 2).toBeLessThan(330);
  expect(box.width / 2).toBeGreaterThan(520);
  const drag = async () => {
    await page.mouse.move(x, y);
    await page.mouse.down();
    await page.mouse.move(x - 350, y, { steps: 6 });
    await page.mouse.up();
  };
  const look = (zoom: number) =>
    page.evaluate(async (zoom) => {
      const still = Reflect.get(window, "still") as Still;
      still.present(1000, 1000, zoom);
      await still.frames(2);
      return still.drawn();
    }, zoom);
  await drag();
  const small = await look(1);
  expect(small.kind).not.toBe("arrow");
  expect(Math.abs(small.x - (x - 350))).toBeLessThan(2);
  expect(Math.abs(small.y - y)).toBeLessThan(2);
  // A whole view, partly on the square and partly on the bar beside it.
  const large = await look(0.1);
  expect(large.kind).toBe("frame");
  expect(Math.abs(large.width - 0.4 * map.width)).toBeLessThan(4);
  expect(Math.abs(large.x - (x - 350))).toBeLessThan(2);
  // Off the bar as well.
  await drag();
  const gone = await look(1);
  expect(gone.kind).toBe("arrow");
  expect(gone.x - x).toBeLessThan(-map.width / 2 - 100);
  expect(gone.x).toBeGreaterThan(box.left);
});

test("the marker is above the map and below the buttons over it", async ({
  page,
}) => {
  await stillMap(page, "native");
  const buttons = ["carry-indicators", "fullscreen", "minimap-zoom"];
  const stacks = await page.evaluate(async (buttons) => {
    const still = Reflect.get(window, "still") as Still;
    // The order of hits is the order of painting, once the marker takes them.
    const style = document.createElement("style");
    style.textContent =
      "#minimap-marker, #minimap-marker * { pointer-events: all !important; }";
    document.head.append(style);
    const stacks = [];
    for (const id of buttons) {
      const box = document.getElementById(id)?.getBoundingClientRect();
      if (!box) throw new Error(`#${id} is missing`);
      const x = box.left + box.width / 2;
      const y = box.top + box.height / 2;
      const { left, top, width, height } = still.map;
      still.present(...still.at(x - left - width / 2, y - top - height / 2));
      await still.frames(2);
      const hits = document
        .elementsFromPoint(x, y)
        .map(
          (node) =>
            node.closest("#minimap-marker,#minimap-map,button")?.id ?? "",
        );
      stacks.push([...new Set(hits)].filter((hit) => hit !== ""));
    }
    return stacks;
  }, buttons);
  expect(stacks.map((stack) => stack.slice(0, 3))).toEqual(
    buttons.map((id) => [id, "minimap-marker", "minimap-map"]),
  );
});

/** The actors of the live client's minimap, once it knows a building. */
async function liveActors(page: Page) {
  await page.goto("/?play=1");
  await expect(page.locator("#connection")).toHaveText("connected");
  const records = await page.evaluate(
    () =>
      new Promise<number[]>((resolve) => {
        const navigation = Reflect.get(window, "aoeNavigation") as {
          actors(records: Float64Array): void;
        };
        const original = navigation.actors;
        navigation.actors = (records) => {
          original(records);
          const kinds = records.filter((_, index) => index % 5 === 0);
          const building = kinds.some((kind) => (kind & 1) === 1);
          const own = kinds.some((kind) => (kind & 8) === 8);
          if (!building || !own) return;
          navigation.actors = original;
          resolve(Array.from(records));
        };
      }),
  );
  const actors = Array.from({ length: records.length / 5 }, (_, index) => ({
    building: !!((records[index * 5] ?? 0) & 1),
    own: !!((records[index * 5] ?? 0) & 8),
    owner: records[index * 5 + 3],
  }));
  const local = actors.find(({ building, own }) => own && !building)?.owner;
  return actors
    .filter(({ building }) => building)
    .map(({ own, owner }) => ({ own, local: owner === local }));
}

test("the client marks its own buildings as its own, like its units", async ({
  page,
}) => {
  await gameAssets(page, true);
  await fiveVillagerFixture(page);
  expect(await liveActors(page)).toEqual([{ own: true, local: true }]);
});

test("the client leaves a rival's building without the mark of its own", async ({
  page,
}) => {
  await gameAssets(page, true);
  // Its town centre belongs to player 3, its scout to the local player 0.
  await ownersFixture(page);
  expect(await liveActors(page)).toEqual([{ own: false, local: false }]);
});
