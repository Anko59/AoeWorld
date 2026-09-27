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
  expect(inputs.version).toBe(2);
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

  // A second anonymous context is a spectator while the controller lease is
  // held. Hand this test's controller session to the Canvas context, using
  // the same resume mechanism as an ordinary reconnect.
  const origin = new URL(page.url()).origin;
  const resumeToken = await page.evaluate(() =>
    sessionStorage.getItem("aoeworld.resume-token"),
  );
  if (!resumeToken)
    throw new Error("visual controller has no resumable session");
  await page.goto("about:blank");
  const canvasPage = await browser.newPage();
  try {
    await canvasPage.addInitScript(
      ({ origin, resumeToken }) => {
        Object.defineProperty(navigator, "gpu", { value: undefined });
        if (
          location.origin === origin &&
          !sessionStorage.getItem("aoeworld.resume-token")
        ) {
          sessionStorage.setItem("aoeworld.resume-token", resumeToken);
        }
      },
      { origin, resumeToken },
    );
    for (const item of [...playableCases, inputs.eviction_case]) {
      await test.step(`${item.location} Canvas`, async () => {
        await captureCase(canvasPage, item, "canvas2d");
      });
    }
  } finally {
    await canvasPage.close();
  }
});
