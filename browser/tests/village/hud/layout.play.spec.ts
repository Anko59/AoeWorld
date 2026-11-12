import { expect } from "@playwright/test";
import { isolatedTest as test } from "../../../server.js";
import { building, inputFixture, selectWorker } from "../input/fixture.js";

for (const selection of ["Villager", "TownCenter"]) {
  test(`${selection} HUD batches canvas geometry reads before panel writes, including after resize`, async ({
    page,
  }, info) => {
    await inputFixture(
      page,
      info.project.name === "webgpu" ? "webgpu" : "webgl2",
      [building(20, 0, 8184, 8184, 4)],
    );
    if (selection === "Villager") await selectWorker(page);
    else {
      const actor = await page.locator("#scene").evaluate((node) => {
        const actors = JSON.parse(node.getAttribute("data-actors") ?? "[]") as {
          kind: string;
          x: number;
          y: number;
        }[];
        return actors.find((actor) => actor.kind === "TownCenter");
      });
      if (!actor) throw new Error("Town Center missing");
      await page.mouse.click(actor.x, actor.y);
    }
    await expect(page.locator("#selection")).toHaveAttribute(
      "data-kind",
      selection,
    );
    const records = await page.evaluate(async () => {
      const canvas = document.getElementById("scene");
      if (!(canvas instanceof HTMLCanvasElement))
        throw new Error("Canvas missing");
      const api = window as unknown as {
        aoeTelemetry: { mark: (section: number) => void };
        aoeInterface: {
          numeric: (values: Float64Array, status: string) => void;
        };
      };
      const numeric = api.aoeInterface.numeric,
        mark = api.aoeTelemetry.mark;
      const originalWidth = canvas.style.width;
      const restore: (() => void)[] = [];
      const rows: {
        before: { name: string; value: number }[];
        after: string[];
      }[] = [];
      let inUi = false,
        written = false;
      let row: (typeof rows)[number] = { before: [], after: [] };
      try {
        for (const name of ["clientWidth", "clientHeight"]) {
          const own = Object.getOwnPropertyDescriptor(canvas, name);
          const getter = Object.getOwnPropertyDescriptor(
            Element.prototype,
            name,
          )?.get;
          if (!getter) throw new Error(`Missing ${name} getter`);
          Object.defineProperty(canvas, name, {
            configurable: true,
            get() {
              const value = getter.call(canvas) as number;
              if (inUi) {
                if (written) row.after.push(name);
                else row.before.push({ name, value });
              }
              return value;
            },
          });
          restore.push(() =>
            own
              ? Object.defineProperty(canvas, name, own)
              : Reflect.deleteProperty(canvas, name),
          );
        }
        await new Promise<void>((resolve) => {
          api.aoeInterface.numeric = (values, status) => {
            numeric(values, status);
            written = true;
          };
          api.aoeTelemetry.mark = (section) => {
            if (section === 12) {
              inUi = true;
              written = false;
              row = { before: [], after: [] };
            }
            mark(section);
            if (section === 19) {
              inUi = false;
              if (written) {
                rows.push(row);
                if (rows.length === 2) canvas.style.width = "50vw";
                if (rows.length === 4) resolve();
              }
            }
          };
        });
        return rows;
      } finally {
        inUi = false;
        api.aoeInterface.numeric = numeric;
        api.aoeTelemetry.mark = mark;
        for (const reset of restore) reset();
        canvas.style.width = originalWidth;
      }
    });
    expect(records).toHaveLength(4);
    for (const row of records) {
      expect(row.before.map((read) => read.name)).toEqual(
        expect.arrayContaining(["clientWidth", "clientHeight"]),
      );
      expect(row.after).toEqual([]);
    }
    const widths = records.map(
      (row) => row.before.find((read) => read.name === "clientWidth")?.value,
    );
    expect(widths[0]).toBeGreaterThan(0);
    expect(widths[3]).toBeLessThan(widths[0] ?? 0);
  });
}
