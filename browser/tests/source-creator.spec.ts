import { expect, test, type Page } from "@playwright/test";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { gameAssets } from "./game-assets.js";

type Profile = "overview" | "detailed";
const profiles: Profile[] = ["overview", "detailed"];
const phase = process.env["AOE_CREATOR_PHASE"];
const evidenceDirectory = new URL("../../reports/creator/", import.meta.url);
const matrixPath = new URL(
  "../../docs/geodata/reference-matrix.json",
  import.meta.url,
);

test.skip(!phase, "Run through make test-creator-source with cached geodata");
test.setTimeout(1_500_000);

const evidenceName = (stem: string, profile: Profile) =>
  profile === "overview" ? stem : `${stem}-${profile}`;

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

async function captureLocatorOverview(page: Page) {
  const overview = page.locator("#map-overview");
  const land = page.locator("#map-land");
  await expect(overview).toBeVisible();
  await expect(land).toBeVisible();
  await expect(land).toHaveAttribute("href", "/locator/world-land.svg");
  const originalViewBox = await overview.getAttribute("viewBox");
  const originalBounds = await land.evaluate((element) => {
    const bounds = (element as SVGImageElement).getBBox();
    return {
      x: bounds.x,
      y: bounds.y,
      width: bounds.width,
      height: bounds.height,
    };
  });
  expect(originalBounds).toEqual({ x: 0, y: 0, width: 360, height: 180 });
  const box = await overview.boundingBox();
  if (!box) throw new Error("world locator overview is missing");
  expect(box.width / box.height).toBeCloseTo(2, 1);
  await overview.click({ position: { x: box.width / 4, y: box.height / 4 } });
  await expect(page.getByLabel("Region")).toHaveValue("custom");
  // Subpixel SVG pointer rounding stays below 0.005°; letterboxing would
  // move this global quarter-point by tens of degrees.
  await expect
    .poll(async () => Number(await page.getByLabel("Latitude").inputValue()))
    .toBeCloseTo(45, 2);
  await expect
    .poll(async () => Number(await page.getByLabel("Longitude").inputValue()))
    .toBeCloseTo(-90, 2);
  await overview.hover({ position: { x: box.width / 2, y: box.height / 2 } });
  await page.mouse.wheel(0, -120);
  await expect(overview).not.toHaveAttribute("viewBox", originalViewBox ?? "");
  const zoomedBounds = await land.evaluate((element) => {
    const bounds = (element as SVGImageElement).getBBox();
    return {
      x: bounds.x,
      y: bounds.y,
      width: bounds.width,
      height: bounds.height,
    };
  });
  expect(zoomedBounds).toEqual(originalBounds);
  await overview.dblclick({
    position: { x: box.width / 2, y: box.height / 2 },
  });
  await expect(overview).toHaveAttribute("viewBox", originalViewBox ?? "");
  return { view_box: originalViewBox, image_bounds: originalBounds };
}

test("ordinary creator prepares and activates overview and detailed source maps", async ({
  page,
  request,
}) => {
  test.skip(phase !== "create", "creation runs only in the first server phase");
  const assetSource = await gameAssets(page);
  const progress = new Map<
    number,
    Array<{ phase: string; completed: number | null; total: number | null }>
  >();
  const requestedChunks = new Set<string>();
  page.on("response", async (response) => {
    const url = new URL(response.url());
    const match = url.pathname.match(/^\/maps\/jobs\/(\d+)$/);
    if (match && response.ok()) {
      const jobId = Number(match[1]);
      const job = await response.json().catch(() => null);
      if (job?.progress?.phase) {
        const current = progress.get(jobId) ?? [];
        current.push({
          phase: job.progress.phase,
          completed: job.progress.completed ?? null,
          total: job.progress.total ?? null,
        });
        progress.set(jobId, current);
      }
    }
  });
  page.on("request", (browserRequest) => {
    const url = new URL(browserRequest.url());
    if (/^\/maps\/[0-9a-f]{64}\/chunks\/\d+\/\d+$/.test(url.pathname))
      requestedChunks.add(url.pathname);
  });

  const locatorResponse = page.waitForResponse((response) => {
    const url = new URL(response.url());
    return url.pathname === "/locator/world-land.svg";
  });
  await page.goto("/");
  await ready(page);
  await page.getByRole("button", { name: "Open map creator" }).click();
  const landResponse = await locatorResponse;
  expect(landResponse.ok()).toBe(true);
  await expect(page.getByLabel("Region")).toHaveValue("paris");
  await expect(page.locator("#map-location")).toContainText(
    "Projection scale error",
  );
  await expect(page.locator("#map-footprint")).toHaveAttribute(
    "visibility",
    "visible",
  );
  const locator = await captureLocatorOverview(page);
  await page.getByLabel("Region").selectOption("paris");
  await expect(page.locator("#map-location")).toContainText("Paris Basin");
  await save(
    "locator-overview",
    { ...locator, asset_status: landResponse.status() },
    page,
  );

  const matrix = JSON.parse(await readFile(matrixPath, "utf8")) as {
    cases: Array<{ id: string; request: Record<string, unknown> }>;
  };
  const paris = matrix.cases.find((item) => item.id === "temperate_inland");
  expect(paris).toBeDefined();

  for (const profile of profiles) {
    if (profile === "detailed") {
      await page.getByRole("button", { name: "Open map creator" }).click();
    }
    await expect(page.getByLabel("Region")).toHaveValue("paris");
    await page.getByLabel("Terrain detail").selectOption(profile);
    await page.getByRole("button", { name: "Estimate" }).click();
    const expectedGrid =
      profile === "detailed" ? "1024 × 1024 samples" : "128 × 128 samples";
    await expect(page.locator("#map-estimate")).toContainText(expectedGrid);
    const estimateText = await page.locator("#map-estimate").textContent();
    if (profile === "detailed") {
      expect(estimateText).toContain("Regional elevation:");
    } else {
      expect(estimateText).toContain("Overview: global elevation");
    }
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
      preparation?: { mode: string; samples_per_axis: number };
    }>;
    const completed = jobs
      .filter(
        (job) => job.state === "completed" && job.preparation?.mode === profile,
      )
      .at(-1);
    if (!completed?.preparation)
      throw new Error(`${profile} creator job metadata is missing`);
    expect(completed?.request).toMatchObject(paris?.request ?? {});
    expect(completed.preparation.mode).toBe(profile);
    expect(completed.preparation.samples_per_axis).toBe(
      profile === "detailed" ? 1024 : 128,
    );
    expect(completed?.content_hash).toMatch(/^[0-9a-f]{64}$/);
    const hash = completed?.content_hash;
    if (!hash)
      throw new Error(`${profile} creator did not report a completed package`);
    expect(
      progress.get(completed.id)?.some((item) => item.completed !== null),
    ).toBe(true);

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
      const captureName =
        profile === "overview"
          ? `preview-${layer}`
          : `preview-detailed-${layer}`;
      const capture = new URL(`${captureName}.png`, evidenceDirectory);
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
    await expect
      .poll(
        () =>
          [...requestedChunks].filter((path) =>
            path.startsWith(`/maps/${hash}/chunks/`),
          ).length,
        { timeout: 30_000 },
      )
      .toBeGreaterThan(0);
    requestedChunks.clear();
    await page.reload();
    await ready(page);
    await expect
      .poll(
        () =>
          [...requestedChunks].filter((path) =>
            path.startsWith(`/maps/${hash}/chunks/`),
          ).length,
        { timeout: 30_000 },
      )
      .toBeGreaterThan(0);
    const joinedWorldChunks = [...requestedChunks]
      .filter((path) => path.startsWith(`/maps/${hash}/chunks/`))
      .sort();
    expect(joinedWorldChunks.length).toBeGreaterThan(0);
    await save(
      evidenceName("created", profile),
      {
        content_hash: hash,
        preparation: {
          mode: profile,
          samples_per_axis: completed.preparation.samples_per_axis,
        },
        request: `Paris Basin · 30 km · 30:1 · 600 CE · ${profile}`,
        estimate: estimateText,
        progress: progress.get(completed.id),
        preview: {
          minimum_height_centimeters: preview.minimum_height_centimeters,
          maximum_height_centimeters: preview.maximum_height_centimeters,
          summary: previewSummary,
          layers: previewLayers,
        },
        source_lock_count: saved?.source_lock_count,
        tiles_per_side: saved?.estimate.tiles_per_side,
        joined_world_chunks: joinedWorldChunks,
        renderer: await page
          .locator("#playground")
          .getAttribute("data-renderer"),
        asset_source: assetSource,
        camera: await page.locator("#world-position").textContent(),
      },
      page,
    );
  }
});

test("overview and detailed packages reopen and load unseen chunks offline", async ({
  page,
  request,
}) => {
  test.skip(phase !== "reopen", "reopen runs only after the server restart");
  const assetSource = await gameAssets(page);
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "gpu", { value: undefined });
  });
  const requestedChunks = new Set<string>();
  page.on("request", (browserRequest) => {
    const url = new URL(browserRequest.url());
    if (/^\/maps\/[0-9a-f]{64}\/chunks\/\d+\/\d+$/.test(url.pathname))
      requestedChunks.add(url.pathname);
  });

  await page.goto("/");
  await ready(page);
  await page.getByRole("button", { name: "Open map creator" }).click();
  for (const profile of profiles) {
    const created = JSON.parse(
      await readFile(
        new URL(`${evidenceName("created", profile)}.json`, evidenceDirectory),
        "utf8",
      ),
    ) as {
      content_hash: string;
      tiles_per_side: number;
      joined_world_chunks: string[];
    };
    const hash = created.content_hash;
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
    await expect
      .poll(
        () =>
          [...requestedChunks].filter((path) =>
            path.startsWith(`/maps/${hash}/chunks/`),
          ).length,
        { timeout: 30_000 },
      )
      .toBeGreaterThan(0);
    const reconnectedChunks = [...requestedChunks]
      .filter((path) => path.startsWith(`/maps/${hash}/chunks/`))
      .sort();
    const chunks = Math.ceil(created.tiles_per_side / 32);
    const unseen = `/maps/${hash}/chunks/0/0`;
    expect(created.joined_world_chunks).not.toContain(unseen);
    expect(reconnectedChunks).not.toContain(unseen);
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
    requestedChunks.clear();
    await page.reload();
    await ready(page);
    await expect
      .poll(
        () =>
          [...requestedChunks].filter((path) =>
            path.startsWith(`/maps/${hash}/chunks/`),
          ).length,
        { timeout: 30_000 },
      )
      .toBeGreaterThan(0);
    const joinedWorldChunks = [...requestedChunks]
      .filter((path) => path.startsWith(`/maps/${hash}/chunks/`))
      .sort();
    await save(
      evidenceName("reopened", profile),
      {
        content_hash: hash,
        preparation: profile,
        offline_unseen_chunk: true,
        unseen_chunk: unseen,
        reconnected_world_chunks: reconnectedChunks,
        joined_world_chunks: joinedWorldChunks,
        renderer: await page
          .locator("#playground")
          .getAttribute("data-renderer"),
        asset_source: assetSource,
        camera: await page.locator("#world-position").textContent(),
        unit_state: await page.locator("#unit-state").textContent(),
      },
      page,
    );
    await page.getByRole("button", { name: "Open map creator" }).click();
  }
});
