import { expect, test, type Page } from "@playwright/test";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { gameAssets } from "./game-assets.js";

const phase = process.env["AOE_CREATOR_PHASE"];
const evidenceDirectory = new URL("../../reports/creator/", import.meta.url);
const createdPath = new URL("created.json", evidenceDirectory);
const matrixPath = new URL(
  "../../docs/geodata/reference-matrix.json",
  import.meta.url,
);

test.skip(!phase, "Run through make test-creator-source with cached geodata");
test.setTimeout(1_200_000);

async function ready(page: Page) {
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-assets",
    "aoe2-local",
  );
}

async function save(name: string, value: Record<string, unknown>, page: Page) {
  await mkdir(evidenceDirectory, { recursive: true });
  const capture = new URL(`${name}.png`, evidenceDirectory);
  await page.screenshot({ path: fileURLToPath(capture), fullPage: true });
  await writeFile(
    new URL(`${name}.json`, evidenceDirectory),
    JSON.stringify({ ...value, capture: fileURLToPath(capture) }, null, 2),
  );
}

test("ordinary creator prepares and activates a source-backed map", async ({
  page,
  request,
}) => {
  test.skip(phase !== "create", "creation runs only in the first server phase");
  const assetSource = await gameAssets(page);
  const progress: Array<{
    phase: string;
    completed: number | null;
    total: number | null;
  }> = [];
  const loadedChunks = new Set<string>();
  page.on("response", async (response) => {
    const url = new URL(response.url());
    if (/^\/maps\/jobs\/\d+$/.test(url.pathname) && response.ok()) {
      const job = await response.json().catch(() => null);
      if (job?.progress?.phase)
        progress.push({
          phase: job.progress.phase,
          completed: job.progress.completed ?? null,
          total: job.progress.total ?? null,
        });
    }
    if (/^\/maps\/[0-9a-f]{64}\/chunks\/\d+\/\d+$/.test(url.pathname))
      loadedChunks.add(url.pathname);
  });
  await page.goto("/");
  await ready(page);
  await page.getByRole("button", { name: "Open map creator" }).click();
  await expect(page.getByLabel("Region")).toHaveValue("paris");
  await expect(page.locator("#map-location")).toContainText(
    "Projection scale error",
  );
  await expect(page.locator("#map-footprint")).toHaveAttribute(
    "visibility",
    "visible",
  );
  await page.getByLabel("Terrain detail").selectOption("overview");
  await page.getByRole("button", { name: "Estimate" }).click();
  await expect(page.locator("#map-estimate")).toContainText(
    "Overview: global elevation",
  );
  const estimate = await page.locator("#map-estimate").textContent();
  await page.getByRole("button", { name: "Generate", exact: true }).click();
  await expect
    .poll(() => page.locator("#map-estimate").textContent(), {
      timeout: 900_000,
      intervals: [500, 1_000, 2_000],
    })
    .toContain("Source-backed package active");
  const jobsResponse = await request.get("/maps/jobs");
  expect(jobsResponse.ok()).toBe(true);
  const jobs = (await jobsResponse.json()) as Array<{
    id: number;
    state: string;
    content_hash?: string;
    request?: Record<string, unknown>;
    progress?: { phase: string; completed?: number; total?: number };
  }>;
  const completed = jobs.filter((job) => job.state === "completed").at(-1);
  const matrix = JSON.parse(await readFile(matrixPath, "utf8")) as {
    cases: Array<{ id: string; request: Record<string, unknown> }>;
  };
  const paris = matrix.cases.find((item) => item.id === "temperate_inland");
  expect(paris).toBeDefined();
  expect(completed?.request).toMatchObject(paris?.request ?? {});
  expect(completed?.content_hash).toMatch(/^[0-9a-f]{64}$/);
  const hash = completed?.content_hash;
  if (!hash) throw new Error("creator did not report a completed package");
  expect(progress.some((item) => item.completed !== null)).toBe(true);
  const packagesResponse = await request.get("/maps");
  expect(packagesResponse.ok()).toBe(true);
  const packages = (await packagesResponse.json()) as Array<{
    content_hash: string;
    source_lock_count: number;
    uses_fallback_data: boolean;
    estimate: { tiles_per_side: number };
  }>;
  const saved = packages.find((item) => item.content_hash === hash);
  expect(saved?.uses_fallback_data).toBe(false);
  expect(saved?.source_lock_count).toBeGreaterThan(0);
  await page.getByLabel("Saved maps").selectOption(hash);
  await page.getByRole("button", { name: "Preview saved" }).click();
  await expect(page.locator("#terrain-preview")).toBeVisible();
  await expect(page.locator("#terrain-preview-summary")).toContainText(
    "coarse 16 × 16 generated terrain sample",
  );
  const previewSummary = await page
    .locator("#terrain-preview-summary")
    .textContent();
  const previewLayers: string[] = [];
  for (const layer of ["elevation", "water", "vegetation", "suitability"]) {
    await page.getByLabel("Terrain preview layer").selectOption(layer);
    const capture = new URL(`preview-${layer}.png`, evidenceDirectory);
    await mkdir(evidenceDirectory, { recursive: true });
    await page
      .locator("#terrain-preview-canvas")
      .screenshot({ path: fileURLToPath(capture) });
    previewLayers.push(fileURLToPath(capture));
  }
  const previewResponse = await request.get(`/maps/${hash}/preview`);
  expect(previewResponse.ok()).toBe(true);
  const preview = (await previewResponse.json()) as {
    source_backed: boolean;
    cells: unknown[];
    minimum_height_centimeters: number;
    maximum_height_centimeters: number;
  };
  expect(preview.source_backed).toBe(true);
  expect(preview.cells).toHaveLength(256);
  expect(preview.maximum_height_centimeters).toBeGreaterThanOrEqual(
    preview.minimum_height_centimeters,
  );
  await page.getByRole("button", { name: "Reconnect to server" }).click();
  await ready(page);
  await page.reload();
  await ready(page);
  await save(
    "created",
    {
      content_hash: hash,
      request: "Paris Basin · 30 km · 30:1 · 600 CE · overview",
      estimate,
      progress,
      preview: {
        minimum_height_centimeters: preview.minimum_height_centimeters,
        maximum_height_centimeters: preview.maximum_height_centimeters,
        summary: previewSummary,
        layers: previewLayers,
      },
      source_lock_count: saved?.source_lock_count,
      tiles_per_side: saved?.estimate.tiles_per_side,
      loaded_chunks: [...loadedChunks].sort(),
      renderer: await page.locator("#playground").getAttribute("data-renderer"),
      asset_source: assetSource,
      camera: await page.locator("#world-position").textContent(),
    },
    page,
  );
});

test("reopen survives server restart and loads an unseen chunk offline", async ({
  page,
  request,
}) => {
  test.skip(phase !== "reopen", "reopen runs only after the server restart");
  const created = JSON.parse(await readFile(createdPath, "utf8")) as {
    content_hash: string;
    tiles_per_side: number;
    loaded_chunks: string[];
  };
  const hash = created.content_hash;
  const assetSource = await gameAssets(page);
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "gpu", { value: undefined });
  });
  await page.goto("/");
  await ready(page);
  await page.getByRole("button", { name: "Open map creator" }).click();
  await page.getByLabel("Saved maps").selectOption(hash);
  await page.getByRole("button", { name: "Open saved" }).click();
  await expect(page.locator("#map-estimate")).toContainText(
    `Saved map active: ${hash.slice(0, 12)}`,
    { timeout: 30_000 },
  );
  await page.getByRole("button", { name: "Reconnect to server" }).click();
  await ready(page);
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    "canvas2d",
  );
  const chunks = Math.ceil(created.tiles_per_side / 32);
  const unseen = `/maps/${hash}/chunks/0/0`;
  expect(created.loaded_chunks).not.toContain(unseen);
  expect(chunks).toBeGreaterThan(1);
  const response = await request.get(unseen);
  expect(response.ok()).toBe(true);
  const chunk = (await response.json()) as { payload_hex?: string };
  expect(chunk.payload_hex?.length).toBeGreaterThan(0);
  const canvas = page.locator("#scene");
  const box = await canvas.boundingBox();
  if (!box) throw new Error("reopened source canvas is missing");
  await canvas.hover({ position: { x: box.width / 2, y: box.height / 2 } });
  await page.mouse.wheel(0, 500);
  await page.reload();
  await ready(page);
  await save(
    "reopened",
    {
      content_hash: hash,
      offline_unseen_chunk: true,
      unseen_chunk: unseen,
      renderer: await page.locator("#playground").getAttribute("data-renderer"),
      asset_source: assetSource,
      camera: await page.locator("#world-position").textContent(),
      unit_state: await page.locator("#unit-state").textContent(),
    },
    page,
  );
});
