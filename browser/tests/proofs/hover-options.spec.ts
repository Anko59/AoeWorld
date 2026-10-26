import { expect } from "@playwright/test";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { PNG } from "pngjs";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";
import { workerFixture } from "../village/fixtures/worker.js";
import { freshHud } from "../village/fixtures/resources.js";

const output = "../.cache/hover-options";
const nodes = [
  { kind: 0, name: "berry", gather: "forage", x: 8191, y: 8194 },
  { kind: 1, name: "tree", gather: "chop", x: 8190, y: 8192 },
  { kind: 2, name: "gold", gather: "mine", x: 8196, y: 8190 },
];

for (const mode of ["cursor", "outline", "glow"] as const) {
  test(`resource hover comparison: ${mode}`, async ({ page }, info) => {
    test.skip(info.project.name !== "webgpu", "WebGPU silhouette comparison");
    test.setTimeout(120_000);
    const capture = await readFile(`${output}/capture.json`, "utf8")
      .then((text) => JSON.parse(text) as { url: string })
      .catch(() => null);
    if (capture) {
      const url = new URL(capture.url);
      expect(url.hostname).toBe("127.0.0.1");
      expect(url.port).toBe("8121");
      await mkdir(output, { recursive: true });
      const health = await page.request.get(new URL("health", url).href);
      expect(health.ok()).toBe(true);
      await writeFile(`${output}/build-${mode}.json`, await health.text());
    } else {
      await gameAssets(page, true);
    }
    await workerFixture(page);
    await page.setViewportSize({ width: 1280, height: 720 });
    const url = capture?.url ?? "/";
    await page.goto(`${url}?play=1&hover=${mode}#x=8192&y=8192&zoom=.8`);
    await expect(page.locator("#connection")).toHaveText("connected", {
      timeout: 60_000,
    });
    await expect(page.locator("#playground")).toHaveAttribute(
      "data-renderer",
      "webgpu",
    );
    await page.evaluate(async (chunks) => {
      const module = await import(
        new URL("/pkg/aoe_client.js", location.href).href
      );
      module.activate_surface_fixture("04".repeat(32), chunks);
    }, JSON.stringify(chunks()));
    // Flat fixture replaces source heights; restore the matching flat focus.
    await page.keyboard.press("Home");
    await freshHud(page);
    const actors = JSON.parse(
      (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
    ) as { id: number; x: number; y: number }[];
    const worker = actors.find((actor) => actor.id === 3);
    if (!worker) throw new Error("Frozen villager missing");
    await page.mouse.click(worker.x, worker.y);
    await expect(page.locator("#selection-name")).toHaveText("Villager");
    for (const node of nodes) {
      // Camera stays at the frozen scout (8192.5,8192.5), flat focus, zoom .8.
      const x = 640 + (node.x - node.y) * 64 * 0.8;
      const y = 360 + (node.x + node.y - 16384) * 32 * 0.8;
      await page.mouse.move(1100, 180);
      await freshHud(page);
      const clip = {
        x: Math.floor(x - 80),
        y: Math.floor(y - 180),
        width: 160,
        height: 210,
      };
      const before = PNG.sync.read(await page.screenshot({ clip }));
      await page.mouse.move(x, y);
      await expect(page.locator("#scene")).toHaveAttribute(
        "data-gather-kind",
        node.gather,
      );
      await expect(page.locator("#scene")).toHaveAttribute(
        "data-resource-hovered",
        "true",
      );
      await expect(page.locator("#scene")).toHaveAttribute(
        "data-hover-mode",
        mode,
      );
      await expect(page.locator("#resource-highlight")).toBeHidden();
      await freshHud(page);
      const after = PNG.sync.read(await page.screenshot({ clip }));
      let changed = 0;
      for (let at = 0; at < before.data.length; at += 4)
        if (
          [0, 1, 2].some(
            (channel) =>
              Math.abs(
                (before.data[at + channel] ?? 0) -
                  (after.data[at + channel] ?? 0),
              ) > 10,
          )
        )
          changed++;
      if (mode === "cursor") expect(changed).toBe(0);
      else
        expect(
          changed,
          `${mode} must outline the ${node.name} sprite body`,
        ).toBeGreaterThan(20);
      if (capture)
        await page.screenshot({ path: `${output}/${mode}-${node.name}.png` });
    }
    await page.mouse.move(1100, 180);
    await freshHud(page);
    await expect(page.locator("#scene")).toHaveAttribute(
      "data-resource-hovered",
      "false",
    );
  });
}

function chunks() {
  const push = (bytes: number[], value: number, count: number) => {
    for (let byte = 0; byte < count; byte++)
      bytes.push(Math.floor(value / 2 ** (byte * 8)) & 255);
  };
  return [
    [255, 255],
    [256, 255],
    [255, 256],
    [256, 256],
  ].map(([cx = 0, cy = 0]) => {
    const resources = nodes.filter(
      (node) =>
        Math.floor(node.x / 32) === cx && Math.floor(node.y / 32) === cy,
    );
    const bytes = [5, 0, 4, resources.length, 0, 0, 0];
    for (let y = 0; y < 32; y++)
      for (let x = 0; x < 32; x++) {
        push(bytes, 0, 4);
        push(bytes, 0, 2);
        for (let corner = 0; corner < 4; corner++) push(bytes, 0, 2);
        push(bytes, 1 << 21, 4);
        push(bytes, 0, 2);
        push(bytes, cx * 32 + x, 4);
        push(bytes, cy * 32 + y, 4);
        for (let absent = 0; absent < 7; absent++) bytes.push(0);
      }
    for (const node of resources) {
      push(bytes, node.y * (1 << 19) + node.x * 2, 8);
      push(bytes, node.x, 4);
      push(bytes, node.y, 4);
      bytes.push(
        node.kind,
        node.kind === 1 ? 0 : node.kind === 0 ? 1 : 2,
        100,
        0,
        0,
        node.kind === 1 ? 2 : 0,
      );
    }
    return {
      x: cx,
      y: cy,
      payload_hex: bytes
        .map((byte) => byte.toString(16).padStart(2, "0"))
        .join(""),
    };
  });
}
