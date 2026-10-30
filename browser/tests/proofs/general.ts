import { expect, type Page } from "@playwright/test";
import { writeFile } from "node:fs/promises";
import { memorySnapshot } from "../memory/exploration-fixture.js";

type Actor = {
  id: number;
  kind: string;
  x: number;
  y: number;
  owned: boolean;
  hp: number;
};

/** Local original-art proof, using real training, selection and march input. */
export async function generalProof(
  page: Page,
  url: string,
  renderer: "canvas" | "webgpu" | "webgl2" = "canvas",
) {
  await page.addInitScript(() => {
    const request = window.requestAnimationFrame.bind(window);
    const samples: number[] = [];
    (window as Window & { generalFrameCpuMs?: number[] }).generalFrameCpuMs =
      samples;
    window.requestAnimationFrame = (callback) =>
      request((time) => {
        const start = performance.now();
        try {
          callback(time);
        } finally {
          if (samples.length === 128) samples.shift();
          samples.push(performance.now() - start);
        }
      });
  });
  expect(
    (
      await page.request.post(`${url}maps/reset?scenario=general-army`)
    ).status(),
  ).toBe(204);
  const loadingStarted = Date.now();
  await page.goto(`${url}?play=1#x=106&y=128&zoom=0.5&renderer=${renderer}`);
  await expect
    .poll(
      async () => {
        const state = await page.locator("#connection").textContent();
        if (state === "Unavailable")
          throw new Error(await page.locator("body").innerText());
        return state;
      },
      { timeout: 60_000 },
    )
    .toBe("connected");
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-village-art",
    "native",
  );
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    renderer === "canvas" ? "canvas2d" : renderer,
  );
  const loadReadyMs = Date.now() - loadingStarted;
  const actors = async (): Promise<Actor[]> =>
    JSON.parse(
      (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
    );
  const jumps: { x: number; y: number }[] = [];
  const started = Date.now();
  const capture = async (name: string) => {
    console.log(`General proof ${name}: ${Date.now() - started} ms`);
    await writeFile(
      `../.cache/ui/general-${name}.json`,
      JSON.stringify(
        {
          actors: await actors(),
          loadReadyMs,
          memory: await memorySnapshot(page),
          // Callback CPU time, including scene preparation/submission; excludes GPU completion.
          frameCpuMs: await page.evaluate(
            () =>
              (window as Window & { generalFrameCpuMs?: number[] })
                .generalFrameCpuMs ?? [],
          ),
          selection: await page.locator("#selection").innerText(),
          status: await page.locator("#vassal-status").textContent(),
          jumps,
          minimap: await page.locator("#minimap-map").boundingBox(),
          camera: await page
            .locator("#minimap")
            .evaluate((e) => ({ ...e.dataset })),
        },
        null,
        2,
      ),
    );
  };
  const select = async (kind: string) => {
    const actor = (await actors()).find((a) => a.kind === kind && a.owned);
    if (!actor) throw new Error(`${kind} missing from the real client`);
    await page.mouse.click(actor.x, actor.y);
    await expect(page.locator("#selection")).toHaveAttribute("data-kind", kind);
  };
  const camera = async (x: number, y: number) => {
    const panel = await page
      .locator("#minimap")
      .evaluate((e) => ({ ...e.dataset }));
    const box = await page.locator("#minimap-map").boundingBox();
    if (!box) throw new Error("Minimap missing");
    const side = Math.min(box.width, box.height);
    const scale = (side * Number(panel.minimapZoom)) / 512;
    const dx = x - Number(panel.cameraX),
      dy = y - Number(panel.cameraY);
    const position = {
      x: Math.round(box.width / 2 + (dx - dy) * scale),
      y: Math.round(box.height / 2 + (dx + dy) * scale),
    };
    jumps.push(position);
    await page.locator("#minimap-map").click({ position });
    await expect
      .poll(async () => {
        const camera = await page
          .locator("#minimap")
          .evaluate((e) => ({ ...e.dataset }));
        return (
          Math.abs(Number(camera.cameraX) - x) +
          Math.abs(Number(camera.cameraY) - y)
        );
      })
      .toBeLessThan(2);
  };
  await select("Castle");
  await capture("castle");
  await page.locator("#train-general").click();
  await expect
    .poll(async () => (await actors()).some((a) => a.kind === "General"), {
      timeout: 25_000,
    })
    .toBe(true);
  await select("General");
  await expect(page.locator("#vassal-status")).toHaveText(
    "General · 14/100 soldiers · idle",
  );
  await capture("command");
  await page.mouse.move(640, 360);
  await page.mouse.wheel(0, 500);
  await page.locator("#minimap-map").hover();
  await page.mouse.wheel(0, -924);
  await expect
    .poll(async () =>
      Number(await page.locator("#minimap").getAttribute("data-minimap-zoom")),
    )
    .toBeGreaterThan(3.9);
  await camera(146, 138);
  // A normal ground order through the enemy group, clear of the HUD.
  await page
    .locator("#scene")
    .click({ position: { x: 1056, y: 488 }, button: "right" });
  const fighting = expect(page.locator("#vassal-status"))
    .toContainText("fighting", {
      timeout: 60_000,
    })
    .then(
      () => null,
      (error) => error,
    );
  await page.waitForTimeout(8000);
  await capture("march-wide");
  await camera(134, 136);
  await page.mouse.move(640, 360);
  await page.mouse.wheel(0, -313);
  await capture("roles");
  await page.waitForTimeout(7000);
  await camera(142, 140);
  await capture("march");
  await page.waitForTimeout(5000);
  await camera(156, 144);
  await capture("engagement");
  expect(await fighting).toBeNull();
  await capture("fight");
  await expect
    .poll(
      async () =>
        (await actors()).filter(
          (a) => !a.owned && a.kind === "Militia" && a.hp > 0,
        ).length,
      {
        timeout: 25_000,
      },
    )
    .toBe(0);
  await capture("victory");
}
