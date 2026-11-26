import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { landPage, layouts, shot, type Land } from "./land-page.js";

test("the plain land cannot shine through fog and the chart ignores the land", async ({
  page,
}) => {
  await landPage(page);
  await page.evaluate(async () => {
    const land = Reflect.get(window, "land") as Land;
    const image = document.getElementById("minimap-terrain");
    const context = document.createElement("canvas").getContext("2d");
    if (!image || !context) throw new Error("The plain land is missing");
    await land.open({ fine: land.raster(64, () => 8 << 4) });
    // Magenta in the plain image, and half of the world unexplored over it.
    context.fillStyle = "#ff00ff";
    context.fillRect(0, 0, 300, 150);
    const loaded = new Promise((resolve) => {
      image.addEventListener("load", resolve, { once: true });
    });
    image.setAttribute("href", context.canvas.toDataURL());
    await loaded;
    land.fog((x) => (x < 128 ? 2 : 0));
    await land.frames(3);
  });
  const magenta = async () => {
    const { image } = await shot(page);
    let count = 0;
    for (let at = 0; at < image.data.length; at += 4) {
      const [red = 0, green = 0, blue = 0] = image.data.subarray(at, at + 3);
      if (red > green + 60 && blue > green + 60) count++;
    }
    return count;
  };
  expect(await magenta()).toBe(0);
  await page.evaluate(async () => {
    const land = Reflect.get(window, "land") as Land;
    land.navigation.fog(true, []);
    await land.frames(3);
  });
  expect(await magenta()).toBeGreaterThan(1000);

  const chart = await page.evaluate(async () => {
    const land = Reflect.get(window, "land") as Land;
    const chart = document.getElementById("minimap-chart");
    if (!chart) throw new Error("#minimap-chart is missing");
    const drawn: string[] = [];
    for (const kind of [2, 11]) {
      await land.open({ fine: land.raster(64, () => kind | (8 << 4)) });
      for (const edge of [0, 90, 200]) {
        land.fog((x) => (x < edge ? 2 : 0));
        await land.frames(2);
        drawn.push(chart.outerHTML);
      }
    }
    const files = 'image,foreignObject,[href]:not([href^="#"])';
    return { drawn, files: chart.querySelectorAll(files).length };
  });
  expect(new Set(chart.drawn).size, "position only").toBe(1);
  expect(chart.files, "no file, no request").toBe(0);
});

for (const layout of layouts) {
  test(`the ${layout} frame lies over the marks and leaves the pointer to the map`, async ({
    page,
  }) => {
    const map = await landPage(page, layout);
    const layers = await page.evaluate(async () => {
      const land = Reflect.get(window, "land") as Land;
      await land.open({ fine: land.raster(64, () => 8 << 4) });
      land.fog((x) => (x < 128 ? 2 : 1));
      await land.frames(2);
      const added = [
        "minimap-chart",
        "minimap-known",
        "minimap-fog",
        "minimap-border",
      ];
      // From the lowest to the topmost; marks may be renamed or removed.
      const found = [
        "minimap-chart",
        "minimap-terrain",
        "minimap-known",
        "minimap-fog",
        "minimap-viewport",
        "minimap-primary",
        "minimap-camera",
        "minimap-resources",
        "minimap-entities",
        "minimap-border",
      ]
        .map((id) => document.getElementById(id))
        .filter((node) => node !== null);
      const misplaced = found.filter((node, at) => {
        const above = found[at + 1];
        const after = Node.DOCUMENT_POSITION_FOLLOWING;
        return above && !(node.compareDocumentPosition(above) & after);
      });
      const taking = added.flatMap((id) => {
        const root = document.getElementById(id);
        const nodes = root ? [root, ...root.querySelectorAll("*")] : [];
        return nodes
          .filter((node) => getComputedStyle(node).pointerEvents !== "none")
          .map((node) => `${id} ${node.tagName}`);
      });
      const terrain = document.getElementById("minimap-terrain");
      // A clip does not shorten a box: the chart's lines end inside the map.
      const chart = document.getElementById("minimap-chart");
      const box = chart?.getBoundingClientRect() ?? new DOMRect();
      const { left, top, width, height } = land.map;
      const spilt = [
        left - box.left,
        top - box.top,
        box.right - left - width,
        box.bottom - top - height,
      ].some((spill) => spill > 1.2);
      return {
        missing: added.filter((id) => !document.getElementById(id)),
        misplaced: misplaced.map((node) => node.id),
        taking,
        next: terrain?.nextElementSibling?.id,
        spilt: chart ? spilt : null,
      };
    });
    expect(layers).toEqual({
      missing: [],
      misplaced: [],
      taking: [],
      next: "minimap-known",
      spilt: false,
    });

    // Under the frame's strokes, inside the lower left side of the world.
    const x = Math.round(map.left + map.width * 0.25);
    const y = Math.round(map.top + map.height * 0.73);
    await page.mouse.click(x, y);
    const jump = await page.evaluate(
      async ([x, y]) => {
        const land = Reflect.get(window, "land") as Land;
        const { minimapWorldPoint } = await import("/minimap.js" as string);
        const view = { zoom: 1, center: [128, 128] };
        // `map` is the drawn rectangle, so every layout fills it.
        const want: number[] = minimapWorldPoint(
          land.map,
          x,
          y,
          [256, 256],
          view,
          true,
        );
        return { want, got: land.jump() };
      },
      [x, y] as const,
    );
    expect(jump.got[0]).toBeCloseTo(jump.want[0] ?? NaN, 2);
    expect(jump.got[1]).toBeCloseTo(jump.want[1] ?? NaN, 2);

    const view = page.locator("#minimap-view");
    const still = await view.getAttribute("transform");
    await page.mouse.move(x, y);
    await page.mouse.down();
    await page.mouse.move(x + 24, y - 12, { steps: 4 });
    await page.mouse.up();
    await expect(view).not.toHaveAttribute("transform", still ?? "");
    const dragged = await page.evaluate(() => {
      const land = Reflect.get(window, "land") as Land;
      return land.jump().map((value) => Number.isNaN(value));
    });
    expect(dragged, "a drag is not a jump").toEqual([true, true]);
  });
}
