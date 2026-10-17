import { expect, type Page } from "@playwright/test";
import { test } from "../server.js";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { gameAssets } from "./game-assets.js";
import { forceCanvas } from "./rendering/backends.js";

type Profile = "overview" | "detailed";
type Job = {
  id: number;
  state: string;
  content_hash: string;
  request: unknown;
  preparation: { mode: string; samples_per_axis: number };
};
const profiles: Profile[] = ["overview", "detailed"];
const phase = process.env["AOE_CREATOR_PHASE"];
const evidenceDirectory = new URL("../../reports/creator/", import.meta.url);
test.skip(!phase, "Run through make test-creator-source with cached geodata");
test.setTimeout(1_500_000);
const name = (stem: string, profile: Profile) =>
  profile === "overview" ? stem : `${stem}-${profile}`;

async function save(stem: string, value: Record<string, unknown>, page: Page) {
  await mkdir(evidenceDirectory, { recursive: true });
  const capture = fileURLToPath(new URL(`${stem}.png`, evidenceDirectory));
  await page.screenshot({ path: capture, fullPage: true });
  await writeFile(
    new URL(`${stem}.json`, evidenceDirectory),
    JSON.stringify({ ...value, capture }, null, 2),
  );
}
async function setup(page: Page) {
  await page.goto("/?skipIntro=1");
  await page
    .getByRole("button", { name: "Single Player", exact: true })
    .click();
  await ready(page);
}
async function ready(page: Page) {
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 30_000,
  });
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-assets",
    "aoe2-local",
  );
}
async function entered(page: Page, hash: string) {
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-map-hash",
    hash,
    { timeout: 900_000 },
  );
  await expect(page.locator("#start-sequence")).toBeHidden({
    timeout: 120_000,
  });
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-chunks-failed",
    "0",
  );
}
function chunks(page: Page) {
  const seen = new Set<string>();
  page.on("request", (request) => {
    const path = new URL(request.url()).pathname;
    if (/^\/maps\/[0-9a-f]{64}\/chunks\/\d+\/\d+$/.test(path)) seen.add(path);
  });
  return seen;
}

test("Single Player setup creates and activates both source preparation profiles", async ({
  page,
  request,
}) => {
  test.skip(phase !== "create", "creation runs only before the server restart");
  const assetSource = await gameAssets(page);
  const requested = chunks(page);
  const progress = new Map<number, unknown[]>();
  page.on("response", async (response) => {
    const match = new URL(response.url()).pathname.match(
      /^\/maps\/jobs\/(\d+)$/,
    );
    if (!match || !response.ok()) return;
    const job = await response.json().catch(() => null);
    if (job?.progress?.completed != null && job?.progress?.phase) {
      const values = progress.get(Number(match[1])) ?? [];
      values.push(job.progress);
      progress.set(Number(match[1]), values);
    }
  });
  const matrix = JSON.parse(
    await readFile(
      new URL("../../docs/geodata/reference-matrix.json", import.meta.url),
      "utf8",
    ),
  ) as { cases: { id: string; request: Record<string, unknown> }[] };
  const paris = matrix.cases.find((value) => value.id === "temperate_inland");
  expect(paris).toBeDefined();
  for (const profile of profiles) {
    // Setup submits to the real worker. Qualify both explicit source profiles
    // while the player-facing setup continues to choose automatic preparation.
    // No server response, package, progress, activation or terrain is mocked.
    await page.route("**/maps/jobs", async (route) => {
      if (route.request().method() !== "POST") return route.continue();
      await route.continue({
        postData: JSON.stringify({
          ...route.request().postDataJSON(),
          preparation: profile,
          hydrology_mode: profile === "detailed" ? "vectors" : "none",
        }),
      });
    });
    await setup(page);
    await page
      .getByRole("button", { name: "Paris basin", exact: true })
      .click();
    await page.locator("#start-size").selectOption("30");
    await page.locator("#start-scale").fill("30");
    await expect(page.locator("#skirmish-estimate")).toContainText("30 km");
    const estimate = await page.locator("#skirmish-estimate").innerText();
    const acknowledgement = page.waitForResponse(
      (response) =>
        new URL(response.url()).pathname === "/maps/jobs" &&
        response.request().method() === "POST",
    );
    await page.getByRole("button", { name: "Start Game", exact: true }).click();
    const accepted = await acknowledgement;
    expect(accepted.ok()).toBe(true);
    const submittedId = ((await accepted.json()) as { id: number }).id;

    await expect(page.locator("#loading-screen")).toBeVisible();
    let completed: Job | undefined;
    await expect
      .poll(
        async () => {
          const response = await request.get("/maps/jobs");
          expect(response.ok()).toBe(true);
          const jobs = (await response.json()) as Job[];
          completed = jobs
            .filter(
              (job) =>
                job.id === submittedId &&
                job.state === "completed" &&
                job.preparation.mode === profile,
            )
            .at(-1);
          return completed?.content_hash;
        },
        { timeout: 900_000, intervals: [500, 1000, 2000] },
      )
      .toMatch(/^[0-9a-f]{64}$/);
    if (!completed) throw new Error("Source job did not complete");
    expect(completed.request).toMatchObject(paris?.request ?? {});
    expect(completed.preparation.samples_per_axis).toBe(1024);
    expect(progress.get(completed.id)?.length).toBeGreaterThan(0);
    const hash = completed.content_hash;
    await entered(page, hash);
    const packagesResponse = await request.get("/maps");
    expect(packagesResponse.ok()).toBe(true);
    const packages = (await packagesResponse.json()) as {
      content_hash: string;
      uses_fallback_data: boolean;
      source_lock_count: number;
      estimate: { tiles_per_side: number };
    }[];
    const saved = packages.find((value) => value.content_hash === hash);
    expect(saved?.uses_fallback_data).toBe(false);
    expect(saved?.source_lock_count).toBeGreaterThan(0);
    const previewResponse = await request.get(`/maps/${hash}/preview`);
    expect(previewResponse.ok()).toBe(true);
    const preview = await previewResponse.json();
    expect(preview.source_backed).toBe(true);
    expect(preview.cells).toHaveLength(256);
    expect(preview.maximum_height_centimeters).toBeGreaterThanOrEqual(
      preview.minimum_height_centimeters,
    );
    requested.clear();
    await page.goto("/?play=1");
    await ready(page);
    await expect(page.locator("#minimap")).toHaveAttribute(
      "data-map-hash",
      hash,
    );
    await expect
      .poll(
        () =>
          [...requested].filter((path) =>
            path.startsWith(`/maps/${hash}/chunks/`),
          ).length,
      )
      .toBeGreaterThan(0);
    await save(
      name("created", profile),
      {
        content_hash: hash,
        job_id: completed.id,
        preparation: completed.preparation,
        request: `Paris Basin · 30 km · 30:1 · 600 CE · ${profile}`,
        estimate,
        progress: progress.get(completed.id),
        preview,
        source_lock_count: saved?.source_lock_count,
        tiles_per_side: saved?.estimate.tiles_per_side,
        joined_world_chunks: [...requested]
          .filter((path) => path.startsWith(`/maps/${hash}/chunks/`))
          .sort(),
        renderer: await page
          .locator("#playground")
          .getAttribute("data-renderer"),
        asset_source: assetSource,
        camera: await page.locator("#world-position").textContent(),
      },
      page,
    );
    await page.unroute("**/maps/jobs");
  }
});

test("setup resumes both completed source jobs after an offline server restart", async ({
  page,
  request,
}) => {
  test.skip(phase !== "reopen", "reopen runs only after the server restart");
  const assetSource = await gameAssets(page);
  await forceCanvas(page);
  const requested = chunks(page);
  for (const profile of profiles) {
    const created = JSON.parse(
      await readFile(
        new URL(`${name("created", profile)}.json`, evidenceDirectory),
        "utf8",
      ),
    ) as {
      content_hash: string;
      job_id: number;
      tiles_per_side: number;
      joined_world_chunks: string[];
    };
    const hash = created.content_hash;
    await page.goto("/diagnostics.html");
    await page.evaluate(
      (id) =>
        localStorage.setItem(
          "aoeworld.map-creation-job",
          JSON.stringify({ id }),
        ),
      created.job_id,
    );
    requested.clear();
    await setup(page);
    await page
      .getByRole("button", { name: "Resume map preparation", exact: true })
      .click();
    await entered(page, hash);
    await expect(page.locator("#playground")).toHaveAttribute(
      "data-renderer",
      "canvas2d",
    );
    await expect
      .poll(
        () =>
          [...requested].filter((path) =>
            path.startsWith(`/maps/${hash}/chunks/`),
          ).length,
      )
      .toBeGreaterThan(0);
    const reconnected = [...requested]
      .filter((path) => path.startsWith(`/maps/${hash}/chunks/`))
      .sort();
    const unseen = `/maps/${hash}/chunks/0/0`;
    expect(created.joined_world_chunks).not.toContain(unseen);
    expect(reconnected).not.toContain(unseen);
    expect(Math.ceil(created.tiles_per_side / 32)).toBeGreaterThan(1);
    const response = await request.get(unseen);
    expect(response.ok()).toBe(true);
    expect((await response.json()).payload_hex?.length).toBeGreaterThan(0);
    requested.clear();
    await page.evaluate(
      (id) =>
        localStorage.setItem(
          "aoeworld.map-creation-job",
          JSON.stringify({ id }),
        ),
      created.job_id,
    );
    await page.reload();
    await page
      .getByRole("button", { name: "Single Player", exact: true })
      .click();
    await page
      .getByRole("button", { name: "Resume map preparation", exact: true })
      .click();
    await entered(page, hash);
    await expect
      .poll(
        () =>
          [...requested].filter((path) =>
            path.startsWith(`/maps/${hash}/chunks/`),
          ).length,
      )
      .toBeGreaterThan(0);
    await save(
      name("reopened", profile),
      {
        content_hash: hash,
        preparation: profile,
        offline_unseen_chunk: true,
        unseen_chunk: unseen,
        reconnected_world_chunks: reconnected,
        joined_world_chunks: [...requested]
          .filter((path) => path.startsWith(`/maps/${hash}/chunks/`))
          .sort(),
        renderer: await page
          .locator("#playground")
          .getAttribute("data-renderer"),
        asset_source: assetSource,
        camera: await page.locator("#world-position").textContent(),
        unit_state: await page.locator("#unit-state").textContent(),
      },
      page,
    );
  }
});
