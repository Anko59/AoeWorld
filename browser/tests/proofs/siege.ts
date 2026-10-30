import { expect, type Page } from "@playwright/test";
import { writeFile } from "node:fs/promises";
import { memorySnapshot } from "../memory/exploration-fixture.js";

type Actor = { kind: string; x: number; y: number; owned: boolean; hp: number };

/** Ordinary encounter orders; actual projectile snapshots and canvas footage. */
export async function siegeProof(page: Page, url: string) {
  expect(
    (
      await page.request.post(`${url}maps/reset?scenario=general-appearance`)
    ).status(),
  ).toBe(204);
  await page.addInitScript(() => {
    const diagnostic = window as unknown as { siegeCpuSamples: number[] };
    diagnostic.siegeCpuSamples = [];
    const request = window.requestAnimationFrame.bind(window);
    window.requestAnimationFrame = (callback) =>
      request((time) => {
        const start = performance.now();
        try {
          callback(time);
        } finally {
          const samples = diagnostic.siegeCpuSamples;
          if (samples.length === 2048) samples.shift();
          samples.push(performance.now() - start);
        }
      });
  });
  const started = Date.now();
  await page.goto(`${url}?play=1#x=135.5&y=128.5&zoom=0.65&renderer=webgpu`);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60_000,
  });
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    "webgpu",
  );
  const actors = async (): Promise<Actor[]> =>
    JSON.parse(
      (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
    );
  await expect
    .poll(async () => {
      const list = await actors();
      return (
        list.some((a) => a.kind === "General") &&
        list.some((a) => a.kind === "BatteringRam" && !a.owned)
      );
    })
    .toBe(true);
  const loadReadyMs = Date.now() - started;
  const cpu = async (clear: boolean) =>
    page.evaluate((clear) => {
      const diagnostic = window as unknown as { siegeCpuSamples: number[] };
      const samples = diagnostic.siegeCpuSamples.slice().sort((a, b) => a - b);
      if (clear) diagnostic.siegeCpuSamples.length = 0;
      return {
        count: samples.length,
        median_ms: samples[Math.floor(samples.length / 2)] ?? null,
        p95_ms: samples[Math.floor(samples.length * 0.95)] ?? null,
      };
    }, clear);
  await cpu(true);
  await page.waitForTimeout(2000);
  const idle = { cpu: await cpu(true), memory: await memorySnapshot(page) };
  const initial = await actors();
  const general = initial.find((a) => a.kind === "General");
  const ram = initial.find((a) => a.kind === "BatteringRam" && !a.owned);
  if (!general || !ram) throw new Error("Siege encounter actors missing");
  await page.mouse.click(general.x, general.y);
  await expect(page.locator("#selection")).toHaveAttribute(
    "data-kind",
    "General",
  );
  await page.mouse.click(ram.x, ram.y, { button: "right" });
  await page.evaluate(() => {
    location.hash = "x=142&y=128&zoom=2&renderer=webgpu";
  });
  await expect(page.locator("#minimap")).toHaveAttribute("data-zoom", "2");
  const shots = async () =>
    Number(await page.locator("#scene").getAttribute("data-projectiles"));
  await expect
    .poll(shots, { timeout: 30_000, intervals: [50] })
    .toBeGreaterThan(0);
  const samples: { count: number; actors: Actor[] }[] = [];
  for (let i = 0; i < 6; i++) {
    samples.push({ count: await shots(), actors: await actors() });
    await page.screenshot({ path: `../.cache/ui/siege-flight-${i}.png` });
    await page.waitForTimeout(100);
  }
  await expect
    .poll(async () => (await actors()).find((a) => a.kind === "General")?.hp, {
      timeout: 60_000,
      intervals: [100],
    })
    .toBe(0);
  await expect.poll(shots, { timeout: 10_000 }).toBe(0);
  await writeFile(
    "../.cache/ui/siege-flight.json",
    JSON.stringify(samples, null, 2),
  );
  await writeFile(
    "../.cache/ui/siege-measurements.json",
    JSON.stringify(
      {
        url,
        load_ready_ms: loadReadyMs,
        idle,
        encounter: {
          cpu: await cpu(false),
          memory: await memorySnapshot(page),
        },
        limits:
          "Single shared-machine software WebGPU observation; callback CPU excludes GPU completion. Instrumentation retains at most 2048 samples. Not dedicated hardware or finished RTS qualification.",
      },
      null,
      2,
    ),
  );
}
