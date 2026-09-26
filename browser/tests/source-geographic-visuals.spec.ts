import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { expect, test } from "@playwright/test";
import { gameAssets } from "./game-assets.js";
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
  expect(inputs.version).toBe(1);
  expect(inputs.activation_cases).toHaveLength(11);
  const assetSource = await gameAssets(page);
  await page.goto("/");
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
  for (const item of inputs.cases) {
    const activation = activations.find((value) => value.case_id === item.id);
    if (!activation?.start_available) {
      continue;
    }
    await test.step(`${item.location} WebGPU`, async () => {
      await captureCase(page, item, "webgpu");
    });
    const canvasPage = await browser.newPage();
    try {
      await canvasPage.addInitScript(() => {
        Object.defineProperty(navigator, "gpu", { value: undefined });
      });
      await test.step(`${item.location} Canvas`, async () => {
        await captureCase(canvasPage, item, "canvas2d");
      });
    } finally {
      await canvasPage.close();
    }
  }
});
