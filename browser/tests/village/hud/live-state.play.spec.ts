import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { inputFixture, selectWorker } from "../input/fixture.js";

test("replaying HUD details preserves live attributes while revisions and changed cursors advance", async ({
  page,
}, info) => {
  await inputFixture(
    page,
    info.project.name === "webgpu" ? "webgpu" : "webgl2",
  );
  await selectWorker(page);
  const result = await page.evaluate(async () => {
    const api = (
      window as unknown as {
        aoeInterface: { details: (values: Float64Array) => void };
      }
    ).aoeInterface;
    const details = api.details;
    const values = await new Promise<Float64Array>((resolve) => {
      api.details = (values) => {
        details(values);
        api.details = details;
        resolve(values.slice());
      };
    });
    const scene = document.getElementById("scene");
    if (!scene) throw new Error("Scene missing");
    const revision = Number(scene.dataset.hudRevision);
    const observer = new MutationObserver(() => {});
    observer.observe(scene, { attributes: true });
    details(values);
    const attributes = observer
      .takeRecords()
      .map((r) => r.attributeName)
      .filter((name) => name !== "data-hud-revision");
    observer.disconnect();
    const nextRevision = Number(scene.dataset.hudRevision);
    values[3] = 3;
    details(values);
    return {
      attributes,
      revision,
      nextRevision,
      cursor: scene.dataset.commandCursor,
    };
  });
  expect(result.attributes).toEqual([]);
  expect(result.nextRevision).toBe(result.revision + 1);
  expect(result.cursor).toBe("forbidden");
});

test("live HUD updates keep unchanged stance and rally attributes without rewriting them", async ({
  page,
}, info) => {
  await inputFixture(
    page,
    info.project.name === "webgpu" ? "webgpu" : "webgl2",
  );
  await selectWorker(page);
  const result = await page.evaluate(async () => {
    const scene = document.getElementById("scene");
    if (!scene) throw new Error("Scene missing");
    const revision = Number(scene.dataset.hudRevision);
    const before = [
      scene.getAttribute("data-selected-stance"),
      scene.getAttribute("data-rally-point"),
    ];
    const writes: string[] = [];
    await new Promise<void>((resolve) => {
      const observer = new MutationObserver((records) => {
        for (const record of records) {
          if (
            record.attributeName === "data-selected-stance" ||
            record.attributeName === "data-rally-point"
          )
            writes.push(record.attributeName);
        }
        if (Number(scene.dataset.hudRevision) >= revision + 4) {
          observer.disconnect();
          resolve();
        }
      });
      observer.observe(scene, { attributes: true });
    });
    return {
      before,
      after: [
        scene.getAttribute("data-selected-stance"),
        scene.getAttribute("data-rally-point"),
      ],
      writes,
    };
  });
  expect(result.before).toEqual(result.after);
  expect(result.writes).toEqual([]);
});
