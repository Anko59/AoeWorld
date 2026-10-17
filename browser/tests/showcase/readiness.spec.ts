import { expect } from "@playwright/test";
import { test } from "../../server.js";

const readiness = await import(
  new URL(
    "../../../crates/harness/src/ship/compare/capture/readiness.mjs",
    import.meta.url,
  ).href
);

const hooks = `<div id="connection">connected</div>
<div id="playground" data-renderer="webgl2"></div>
<div id="minimap" data-camera-x="100" data-camera-y="100" data-zoom="1" data-map-hash="map"></div>
<canvas id="scene" data-chunks-pending="0" data-chunks-failed="0" data-frame="1"></canvas>`;
const view = { x: 100, y: 100, zoom: 1 };

test("showcase waits for injected pending hooks and late redraws", async ({
  page,
}) => {
  await page.setContent(hooks);
  await page.evaluate(() => {
    const scene = document.getElementById("scene");
    if (!scene) throw new Error("missing scene hook");
    scene.dataset.chunksPending = "2";
    scene.dataset.chunksFailed = "2";
    setTimeout(() => {
      scene.dataset.chunksPending = "0";
      scene.dataset.chunksFailed = "0";
      scene.dataset.frame = "2";
    }, 500);
    setTimeout(() => {
      scene.dataset.frame = "3";
    }, 1000);
  });
  const start = Date.now();
  const result = await readiness.waitForView(page, view);
  expect(Date.now() - start).toBeGreaterThanOrEqual(2500);
  expect(result.history.at(-1).frame).toBe(3);
  expect(readiness.quiet(result.history)).toBe(true);
  expect(result.map_hash).toBe("map");
});

test("showcase adapts its settle window to slow frames", async ({ page }) => {
  await page.setContent(hooks);
  await page.evaluate(() => {
    const scene = document.getElementById("scene");
    if (!scene) throw new Error("missing scene hook");
    for (let i = 1; i <= 4; i++) {
      setTimeout(() => {
        scene.dataset.frame = String(i + 1);
      }, i * 500);
    }
  });
  const result = await readiness.waitForView(page, view);
  expect(readiness.windowMs(result.history)).toBeGreaterThanOrEqual(1900);
  expect(result.history.at(-1).frame).toBe(5);
});

test("missing presentation hooks never look settled", async ({ page }) => {
  await page.setContent(hooks);
  await page
    .locator("#scene")
    .evaluate((scene) => scene.removeAttribute("data-frame"));
  const sample = await readiness.sample(page);
  expect(sample.frame).toBe(-1);
  expect(readiness.quiet([sample, { ...sample, at: sample.at + 10000 }])).toBe(
    false,
  );
});

test("failed hooks retain compare's twelve second retry allowance", async ({
  page,
}) => {
  await page.setContent(
    hooks
      .replace('data-chunks-pending="0"', 'data-chunks-pending="3"')
      .replace('data-chunks-failed="0"', 'data-chunks-failed="3"'),
  );
  const sample = await readiness.sample(page);
  const history = [sample, { ...sample, at: sample.at + 11999 }];
  expect(readiness.failingFor(history)).toBe(11999);
  history.push({ ...sample, at: sample.at + 12000 });
  expect(readiness.failingFor(history)).toBe(12000);
  expect(readiness.quiet(history)).toBe(false);
});

test("wrong cameras and failed chunks cannot settle successfully", async ({
  page,
}) => {
  await page.setContent(hooks);
  const sample = await readiness.sample(page);
  expect(readiness.landed([sample], { ...view, x: 101 })).toBe(false);
  const failed = { ...sample, failed: 1 };
  expect(readiness.quiet([failed, { ...failed, at: failed.at + 5000 }])).toBe(
    false,
  );
});

test("camera scenes reuse one app document and renderer changes reload", async ({
  page,
}) => {
  let loads = 0;
  await page.route("http://127.0.0.1:8999/**", async (route) => {
    loads++;
    await route.fulfill({ contentType: "text/html", body: hooks });
  });
  const first = "http://127.0.0.1:8999/#x=100&renderer=webgl2";
  const second = "http://127.0.0.1:8999/#x=210&renderer=webgl2";
  await page.goto(first);
  await page.evaluate(() => {
    document.body.dataset.instance = "one";
  });
  expect(readiness.sameCameraDocument(page.url(), second)).toBe(true);
  await page.evaluate((url) => {
    location.hash = new URL(url).hash;
  }, second);
  expect(loads).toBe(1);
  expect(await page.locator("body").getAttribute("data-instance")).toBe("one");
  expect(
    readiness.sameCameraDocument(second, second.replace("webgl2", "webgpu")),
  ).toBe(false);
  expect(
    readiness.sameCameraDocument(second, second.replace("8999", "8998")),
  ).toBe(false);
});
