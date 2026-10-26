import { expect, type Page, type Route } from "@playwright/test";
import { test } from "../../server.js";
import { gameAssets } from "../game-assets.js";

type Navigation = { map(hash: Uint8Array): void };

async function openMap(page: Page): Promise<string> {
  expect((await page.request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page);
  await page.goto("/#x=100&y=100&zoom=1");
  await expect(page.locator("#connection")).toHaveText("connected");
  const hash = process.env["AOE_E2E_LANDSCAPE_HASH"];
  if (!hash)
    throw new Error("The disposable E2E stack supplies a prepared map");
  await page.evaluate(async (hash) => {
    const response = await fetch(`/maps/${hash}`, {
      method: "POST",
      headers: {
        "x-aoeworld-controller-token":
          localStorage.getItem("aoeworld.resume-token") ?? "",
      },
    });
    if (!response.ok) throw new Error(await response.text());
  }, hash);
  await expect(page.locator("#connection")).toHaveText(/reconnect/);
  await page.getByRole("button", { name: "Menu", exact: true }).click();
  await page.getByRole("button", { name: "Reconnect to server" }).click();
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect(page.locator("#minimap")).toHaveAttribute("data-map-hash", hash);
  return hash;
}

async function snapshot(page: Page, label: string) {
  console.log(
    `${label}: data-raster=${await page.locator("#minimap").getAttribute("data-raster")}; ${await page.locator("#minimap-title").innerText()}`,
  );
}

test("minimap raster readiness waits for terrain and resets when the map changes", async ({
  page,
}) => {
  let rasterRoute: Route | undefined;
  await page.route("**/maps/*/minimap", (route) => {
    rasterRoute = route;
  });
  await openMap(page);
  await expect.poll(() => !!rasterRoute).toBe(true);
  await snapshot(page, "requested");
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-raster",
    "pending",
  );
  await expect(page.locator("#minimap-title")).toHaveText(
    "Minimap · schematic",
  );
  if (!rasterRoute) throw new Error("Raster request missing");
  await rasterRoute.continue();
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-raster",
    "ready",
  );
  await expect(page.locator("#minimap-title")).toHaveText("Minimap · terrain");
  await expect(page.locator("#minimap")).toHaveAttribute(
    "data-raster-samples",
    /^[1-9]\d*$/,
  );
  await expect(page.locator("#minimap-terrain")).toHaveAttribute(
    "href",
    /^data:image\/png/,
  );
  await snapshot(page, "decoded and drawn");
  const reset = await page.evaluate(async () => {
    const panel = document.getElementById("minimap");
    if (!panel) throw new Error("Minimap missing");
    const writes: (string | null)[] = [];
    const observer = new MutationObserver((records) => {
      writes.push(...records.map((record) => record.oldValue));
    });
    observer.observe(panel, {
      attributes: true,
      attributeFilter: ["data-raster"],
      attributeOldValue: true,
    });
    const navigation = (window as Window & { aoeNavigation?: Navigation })
      .aoeNavigation;
    if (!navigation) throw new Error("Production navigation bridge missing");
    const hash = panel.dataset.mapHash;
    if (!hash) throw new Error("Map hash missing");
    const bytes = Uint8Array.from(hash.match(/../g) ?? [], (byte) =>
      parseInt(byte, 16),
    );
    navigation.map(bytes);
    navigation.map(new Uint8Array());
    navigation.map(new Uint8Array());
    await Promise.resolve();
    observer.disconnect();
    return {
      writes,
      raster: panel.dataset.raster,
      hash: panel.dataset.mapHash,
      image: document.getElementById("minimap-terrain")?.getAttribute("href"),
      title: document.getElementById("minimap-title")?.textContent,
    };
  });
  expect(reset).toEqual({
    writes: ["ready"],
    raster: "pending",
    hash: "",
    image: null,
    title: "Minimap · schematic",
  });
  await snapshot(page, "map cleared");
});

for (const failure of ["network", "http", "decode"] as const) {
  test(`minimap raster reports failed after ${failure} failure`, async ({
    page,
  }) => {
    await page.route("**/maps/*/minimap", async (route) => {
      if (failure === "network") await route.abort("failed");
      else if (failure === "http") await route.fulfill({ status: 503 });
      else
        await route.fulfill({
          contentType: "application/octet-stream",
          body: "invalid raster",
        });
    });
    await openMap(page);
    await snapshot(page, `${failure} failure`);
    await expect(page.locator("#minimap")).toHaveAttribute(
      "data-raster",
      "failed",
    );
    await expect(page.locator("#minimap-title")).toHaveText(
      "Minimap · schematic",
    );
    await expect(page.locator("#minimap-terrain")).not.toHaveAttribute(
      "href",
      /.+/,
    );
    await snapshot(page, `${failure} result`);
  });
}
