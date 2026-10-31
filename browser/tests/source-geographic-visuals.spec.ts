import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { expect } from "@playwright/test";
import { test } from "../server.js";
import { gameAssets } from "./game-assets.js";
import { forceCanvas } from "./rendering/backends.js";
import {
  activateMatrixCase,
  assetIdentity,
  caseInputsPath,
  captureCase,
  evidenceDirectory,
  ready,
  type ActivationOutcome,
  type CaptureInputs,
} from "./source-geographic-visual-capture.js";

test.skip(
  !process.env["AOE_SOURCE_VISUAL_CASES"],
  "Run through make test-geographic-visuals with a verified matrix",
);
test.setTimeout(1_200_000);

test("source-backed water and high-relief maps render and remain navigable", async ({
  page,
  browser,
}) => {
  const inputs = JSON.parse(
    await readFile(caseInputsPath(), "utf8"),
  ) as CaptureInputs;
  expect(inputs.version).toBe(2);
  expect(inputs.activation_cases).toHaveLength(11);
  const assetSource = await gameAssets(page);
  await page.goto("/?play=1");
  const asset = await assetIdentity(page);
  await ready(page);
  const activations: ActivationOutcome[] = [];
  for (const item of inputs.activation_cases) {
    const outcome = await test.step(`${item.location} activation`, async () =>
      activateMatrixCase(page, item, assetSource, asset, inputs));
    activations.push(outcome);
  }
  await writeFile(
    fileURLToPath(new URL("activation.json", evidenceDirectory)),
    JSON.stringify(
      {
        version: 1,
        capture_revision: inputs.capture_revision,
        prepared_revision: inputs.prepared_revision,
        cases: activations,
      },
      null,
      2,
    ),
  );
  expect(inputs.cases.map((item) => item.id)).toEqual([
    "river_lake",
    "nile_delta_coast",
    "alpine_relief",
  ]);
  const playableCases = inputs.cases.filter((item) =>
    activations.some(
      (value) => value.case_id === item.id && value.start_available,
    ),
  );
  const alpine = activations.find((value) => value.case_id === "alpine_relief");
  expect(alpine?.start_available).toBe(true);
  expect(inputs.eviction_case.id).toBe("alpine_eviction");
  expect(inputs.eviction_case.tiles_per_side).toBe(750);
  expect(inputs.eviction_case.package_chunk_count_bound).toBe(576);
  for (const item of [...playableCases, inputs.eviction_case]) {
    await test.step(`${item.location} WebGPU`, async () => {
      await captureCase(page, item, "webgpu");
    });
  }

  // The Canvas page is a full player of its own: no session hand-off.
  await page.goto("about:blank");
  const canvasPage = await browser.newPage();
  try {
    await forceCanvas(canvasPage);
    for (const item of [...playableCases, inputs.eviction_case]) {
      await test.step(`${item.location} Canvas`, async () => {
        await captureCase(canvasPage, item, "canvas2d");
      });
    }
  } finally {
    await canvasPage.close();
  }
});
