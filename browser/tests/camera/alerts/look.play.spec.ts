import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { pixels, type Still } from "../still-map.js";
import {
  type Alerts,
  type Place,
  alertMap,
  blacken,
  expectStars,
  hit,
  run,
  spot,
  stars,
} from "./alert-page.js";

for (const [frame, size] of [
  ["native", 20],
  ["square", 20],
  ["overview", 30],
] as const)
  test(`a star of the ${frame} map is about ${size} px wide and a ring beats three times around it`, async ({
    page,
  }) => {
    await alertMap(page, frame, "no-preference");
    const place = await spot(page, 20, 10);
    // Held in the task that draws the star, however slow this machine is.
    const seen = await page.evaluate((world) => {
      const navigation = Reflect.get(window, "aoeNavigation") as {
        hits(places: Float64Array): void;
      };
      const still = Reflect.get(window, "still") as Still;
      const { stars, moving } = Reflect.get(window, "alerts") as Alerts;
      navigation.hits(Float64Array.from(world));
      still.present(100, 100);
      const beats = moving("[data-minimap-alert]");
      for (const beat of beats) beat.pause();
      const at = (time: number) => {
        for (const beat of beats) beat.currentTime = time;
        return stars();
      };
      const ends = beats.map(({ effect }) =>
        Number(effect?.getComputedTiming().endTime),
      );
      return {
        ends,
        beats: [800, 1_700, 2_600].map((time) => at(time)),
        rest: at(Math.max(0, ...ends)),
      };
    }, place.world);
    const last = Math.max(...seen.ends);
    expect(last).toBeGreaterThan(2_400);
    expect(last).toBeLessThan(3_000);
    // The ring grows from the star's own radius to 2.2 times that, in 900 ms.
    for (const [star] of seen.beats) {
      expect(star?.width).toBeGreaterThan(size * 1.45);
      expect(Math.abs((star?.x ?? NaN) - place.x)).toBeLessThan(1.5);
    }
    const [star] = seen.rest;
    expect(seen.rest).toHaveLength(1);
    expect(star?.width).toBeGreaterThan(size * 0.9);
    expect(star?.width).toBeLessThan(size * 1.2);
    // The native map is stretched; a star is not.
    expect(Math.abs((star?.width ?? NaN) - (star?.height ?? NaN))).toBeLessThan(
      1,
    );
  });

test("reduced motion shows the star alone: eight points in orange and ivory around what is fought over", async ({
  page,
}) => {
  await alertMap(page, "native");
  await blacken(page);
  const place = await spot(page, 20, 10);
  await page.evaluate(
    ([x, y]) => {
      const still = Reflect.get(window, "still") as Still;
      const { left, top, width, height } = still.map;
      still.actors(
        still.block([x - left - width / 2, y - top - height / 2], [4, 4], {
          kind: 8,
          owner: 1,
          colour: 0xff00ff,
        }),
      );
    },
    [place.x, place.y] as const,
  );
  await hit(page, place.world);
  await run(page, 50);
  const moving = await page.evaluate(() => {
    const { moving } = Reflect.get(window, "alerts") as Alerts;
    return moving("[data-minimap-alert]").length;
  });
  expect(moving).toBe(0);
  const [first] = await stars(page);
  expect(first?.width).toBeLessThan(24);
  await run(page, 3_000);
  expect(await stars(page)).toEqual([first]);

  const tips = await page.evaluate(() =>
    Array.from(
      document.querySelectorAll<SVGGeometryElement>(
        "[data-minimap-alert]:is(path,polygon), [data-minimap-alert] :is(path,polygon)",
      ),
      (outline) => {
        const box = outline.getBBox();
        const length = outline.getTotalLength();
        const radii = Array.from({ length: 720 }, (_, step) => {
          const { x, y } = outline.getPointAtLength((step / 720) * length);
          return Math.hypot(
            x - box.x - box.width / 2,
            y - box.y - box.height / 2,
          );
        });
        const most = Math.max(...radii) * 0.97;
        return radii.filter(
          (radius, step) =>
            radius > most &&
            radius >= (radii.at(step - 1) ?? 0) &&
            radius > (radii[(step + 1) % 720] ?? 0),
        ).length;
      },
    ),
  );
  expect(tips).toContain(8);

  const colour = await pixels(page);
  const centre = colour(place.x, place.y);
  expect(centre.red, "the units in the hollow star").toBeGreaterThan(190);
  expect(centre.blue).toBeGreaterThan(190);
  expect(centre.green).toBeLessThan(110);
  let orange = 0;
  let ivory = 0;
  for (let across = -13; across <= 13; across++)
    for (let down = -13; down <= 13; down++) {
      const apart = Math.hypot(across, down);
      if (apart < 6.5 || apart > 13) continue;
      const { red, green, blue } = colour(place.x + across, place.y + down);
      if (red > 190 && green > 40 && green < 160 && blue < 120) orange++;
      if (Math.min(red, green, blue) > 130) ivory++;
    }
  expect(orange).toBeGreaterThan(10);
  expect(ivory).toBeGreaterThan(8);
});

test("a star is drawn over the marks, the frame and an order's ping, and under the view marker", async ({
  page,
}) => {
  await alertMap(page, "native");
  const seen = await page.evaluate(() => {
    const still = Reflect.get(window, "still") as Still;
    const navigation = Reflect.get(window, "aoeNavigation") as {
      hits(places: Float64Array): void;
      bindOrders(send: () => number): void;
    };
    const { map } = still;
    const [x, y] = [
      map.left + map.width / 2 + 46,
      map.top + map.height / 2 + 20,
    ] as const;
    // The star stands 12 px beside the camera: 70 tiles off its screen.
    const camera = still.at(40, 20);
    still.actors(
      still.block([46, 20], [30, 20], { kind: 8, owner: 1, colour: 0x0000ff }),
    );
    still.present(...camera);
    navigation.bindOrders(() => 2);
    document.getElementById("minimap-map")?.dispatchEvent(
      new PointerEvent("pointerdown", {
        bubbles: true,
        cancelable: true,
        button: 2,
        clientX: x,
        clientY: y,
      }),
    );
    navigation.hits(Float64Array.from(still.at(52, 20)));
    still.present(...camera);

    // The order of hits is the order of painting, once all of it takes them.
    const style = document.createElement("style");
    style.textContent =
      "#minimap-marker, #minimap-marker *, #minimap-marks *, [data-minimap-alert], [data-minimap-alert] * { pointer-events: all !important; }";
    document.head.append(style);
    const layers =
      "#minimap-marker,[data-minimap-ping],[data-minimap-alert],#minimap-marks";
    const stack = document.elementsFromPoint(x, y).flatMap((node) => {
      const layer = node.closest(layers);
      if (!layer || layer.matches("[data-minimap-ping]")) return [];
      return [layer.matches("[data-minimap-alert]") ? "star" : layer.id];
    });
    const ping = document.querySelector("[data-minimap-ping]");
    const star = document.querySelector("[data-minimap-alert]");
    const border = document.getElementById("minimap-border");
    const mark = [...document.querySelectorAll("#minimap-marks path")]
      .filter((node) => node.closest(layers)?.id === "minimap-marks")
      .at(-1);
    if (!ping || !star || !mark || !border)
      throw new Error("A layer is missing");
    const { DOCUMENT_POSITION_FOLLOWING: after } = Node;
    const follows = (first: Element, then: Element) =>
      (first.compareDocumentPosition(then) & after) !== 0;
    return {
      stack: [...new Set(stack)],
      order: [
        follows(mark, border),
        follows(border, ping),
        follows(ping, star),
      ],
    };
  });
  expect(seen.stack).toEqual(["minimap-marker", "star", "minimap-marks"]);
  expect(seen.order, "marks, frame, ping, star").toEqual([true, true, true]);
});

test("the minimap is not written while nothing is hit nor while a star rests", async ({
  page,
}) => {
  await alertMap(page, "native");
  const records = await page.evaluate(() => {
    const still = Reflect.get(window, "still") as Still;
    const records = still.block([-60, 10], [12, 8], {
      kind: 8,
      owner: 1,
      colour: 0x0000ff,
    });
    still.actors(records);
    const minimap = document.getElementById("minimap");
    if (!minimap) throw new Error("#minimap is missing");
    const changes: string[] = [];
    const keep = (list: MutationRecord[]) => {
      for (const { type, target, attributeName } of list) {
        const node = target instanceof Element ? target : target.parentElement;
        // Words for a screen reader come and go in their own time.
        if (node?.closest('[aria-live],[role="status"]')) continue;
        changes.push(`${type} ${attributeName ?? ""} of ${target.nodeName}`);
      }
    };
    const observer = new MutationObserver(keep);
    observer.observe(minimap, {
      attributes: true,
      characterData: true,
      childList: true,
      subtree: true,
    });
    Reflect.set(window, "written", () => {
      keep(observer.takeRecords());
      return changes.splice(0);
    });
    return records;
  });
  const written = () =>
    page.evaluate(() => (Reflect.get(window, "written") as () => string[])());
  // Sixty frames of a still camera over a second and a half.
  const idle = async () => {
    const seen: string[] = [];
    for (let part = 0; part < 3; part++) {
      await page.clock.runFor(500);
      seen.push(
        ...(await page.evaluate((records) => {
          const still = Reflect.get(window, "still") as Still;
          for (let frame = 0; frame < 20; frame++) {
            still.present(100, 100);
            still.actors(records);
          }
          return (Reflect.get(window, "written") as () => string[])();
        }, records)),
      );
    }
    return seen;
  };
  await written();
  expect(await idle(), "before a hit").toEqual([]);
  const place: Place = [1200, 1000];
  await hit(page, place);
  await run(page, 1_000);
  expect((await written()).length).toBeGreaterThan(0);
  expect(await stars(page)).toHaveLength(1);
  expect(await idle(), "under a resting star").toEqual([]);
  expect(await stars(page)).toHaveLength(1);
  await run(page, 8_000);
  await expectStars(page, []);
  await written();
  expect(await idle(), "after the star").toEqual([]);
});
