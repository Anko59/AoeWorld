import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { inputFixture, selectWorker } from "../input/fixture.js";

test("unchanged command panels retain their DOM and selection contents", async ({
  page,
}, info) => {
  await inputFixture(
    page,
    info.project.name === "webgpu" ? "webgpu" : "webgl2",
  );
  await selectWorker(page);
  const result = await page.evaluate(async () => {
    const api = window as unknown as {
      aoeInterface: { numeric: (values: Float64Array, status: string) => void };
      aoeBuildings: { update: (values: Float64Array) => void };
    };
    let numeric: [Float64Array, string] | undefined,
      buildings: Float64Array | undefined;
    const originalNumeric = api.aoeInterface.numeric,
      originalBuildings = api.aoeBuildings.update;
    try {
      api.aoeInterface.numeric = (values, status) => {
        numeric = [values.slice(), status];
        originalNumeric(values, status);
      };
      api.aoeBuildings.update = (values) => {
        buildings = values.slice();
        originalBuildings(values);
      };
      await new Promise<void>((resolve) => {
        const wait = () =>
          numeric && buildings ? resolve() : requestAnimationFrame(wait);
        wait();
      });
      const panel = document.getElementById("bottom-panel");
      if (!panel || !numeric || !buildings)
        throw new Error("HUD snapshot missing");
      const before = panel.innerHTML;
      const observer = new MutationObserver(() => {});
      observer.observe(panel, {
        subtree: true,
        attributes: true,
        characterData: true,
        childList: true,
      });
      originalNumeric(...numeric);
      originalBuildings(buildings);
      const changes = observer.takeRecords().map((record) => ({
        id: (record.target as Element).id,
        attribute: record.attributeName,
        type: record.type,
      }));
      observer.disconnect();
      return { before, after: panel.innerHTML, changes };
    } finally {
      api.aoeInterface.numeric = originalNumeric;
      api.aoeBuildings.update = originalBuildings;
    }
  });
  expect(result.after).toBe(result.before);
  expect(result.changes).toEqual([]);
  await expect(page.locator("#selection-name")).toHaveText("Villager");
  // A changed selection must still refresh the same retained panel.
  await page.keyboard.press("Escape");
  await expect(page.locator("#selection-content")).toBeHidden();
});

test("moving actor labels retain nodes while positions, health and loads change", async ({
  page,
}, info) => {
  await inputFixture(
    page,
    info.project.name === "webgpu" ? "webgpu" : "webgl2",
    [],
    false,
    0,
    undefined,
    { carried: true },
  );
  await selectWorker(page);
  const result = await page.evaluate(async () => {
    const api = (
      window as unknown as {
        aoeInterface: {
          numeric: (values: Float64Array, status: string) => void;
        };
      }
    ).aoeInterface;
    const numeric = api.numeric;
    const [values, status] = await new Promise<[Float64Array, string]>(
      (resolve) => {
        api.numeric = (v, s) => {
          numeric(v, s);
          api.numeric = numeric;
          resolve([v.slice(), s]);
        };
      },
    );
    const bar = document.querySelector<HTMLElement>(
      "#unit-health-bars .unit-hp-bar",
    );
    const load = document.querySelector<HTMLElement>(
      "#carried-loads .load-badge",
    );
    if (!bar || !load) throw new Error("Initial actor labels missing");
    const before = { health: bar.outerHTML, load: load.outerHTML };
    const selected = Array.from(
      { length: values[12] ?? 0 },
      (_, i) => 17 + i * 19,
    ).find((at) => values[at + 4] === 1);
    if (selected === undefined) throw new Error("Selected actor missing");
    const change = (offset: number, delta: number) => {
      const value = values[selected + offset];
      if (value === undefined) throw new Error("Incomplete actor record");
      values[selected + offset] = value + delta;
    };
    change(2, 9);
    change(7, 1);
    change(8, -1);
    change(13, 7);
    numeric(values, status);
    const retained =
      bar === document.querySelector("#unit-health-bars .unit-hp-bar") &&
      load === document.querySelector("#carried-loads .load-badge");
    const movedBar = document.querySelector("#unit-health-bars .unit-hp-bar");
    const movedLoad = document.querySelector("#carried-loads .load-badge");
    if (!movedBar || !movedLoad) throw new Error("Moved actor labels missing");
    const after = { health: movedBar.outerHTML, load: movedLoad.outerHTML };
    values[selected + 8] = 0;
    numeric(values, status);
    return {
      before,
      after,
      retained,
      removed: !bar.isConnected && !load.isConnected,
    };
  });
  expect(result.after.health).not.toBe(result.before.health);
  expect(result.after.load).not.toBe(result.before.load);
  expect(result.retained).toBe(true);
  expect(result.removed).toBe(true);
});
