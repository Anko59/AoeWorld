import { expect } from "@playwright/test";
import { test } from "../../server.js";

const readiness = await import(
  new URL(
    "../../../crates/harness/src/ship/compare/capture/readiness.mjs",
    import.meta.url,
  ).href
);

const first: { kind: string; url: string; steps?: unknown[] } = {
  kind: "browser",
  url: "http://127.0.0.1:8999/#x=100&y=100&zoom=1&renderer=webgl2",
};

for (const scenario of [
  {
    name: "renderer changes reload and initialize the requested renderer",
    previous: first,
    target: { ...first, url: first.url.replace("webgl2", "webgpu") },
    loads: 2,
  },
  {
    name: "interaction followed by the same bookmark reloads its camera",
    previous: { ...first, steps: [{ wheel: { delta_y: -400 } }] },
    target: first,
    loads: 2,
  },
  {
    name: "interaction followed by a different bookmark also reloads",
    previous: { ...first, steps: [{ key: "ArrowRight" }] },
    target: { ...first, url: first.url.replace("x=100", "x=210") },
    loads: 2,
  },
  {
    name: "plain consecutive cameras with the same renderer reuse the document",
    previous: { ...first, steps: [] },
    target: { ...first, url: first.url.replace("x=100", "x=210") },
    loads: 1,
  },
  {
    name: "identical fragments reload to reapply the bookmark",
    previous: first,
    target: first,
    loads: 2,
  },
]) {
  test(scenario.name, async ({ page }) => {
    let loads = 0;
    await page.route("http://127.0.0.1:8999/**", async (route) => {
      loads++;
      await route.fulfill({
        contentType: "text/html",
        body: `<script>
          const fragment = () => new URLSearchParams(location.hash.slice(1));
          document.documentElement.dataset.renderer = fragment().get("renderer");
          const camera = () => {
            document.documentElement.dataset.x = fragment().get("x");
            document.documentElement.dataset.zoom = fragment().get("zoom");
          };
          camera();
          addEventListener("hashchange", camera);
          addEventListener("wheel", () => {
            document.documentElement.dataset.zoom = "1.822";
          });
        </script>`,
      });
    });
    await page.goto(first.url);
    await page.evaluate(() => {
      document.documentElement.dataset.instance = "original";
    });
    if (scenario.previous.steps?.length) {
      await page.evaluate(() => dispatchEvent(new WheelEvent("wheel")));
      await expect(page.locator("html")).toHaveAttribute("data-zoom", "1.822");
    }

    await readiness.navigateScene(page, scenario.target, scenario.previous);

    expect(loads).toBe(scenario.loads);
    const target = new URLSearchParams(
      new URL(scenario.target.url).hash.slice(1),
    );
    const renderer = target.get("renderer");
    const x = target.get("x");
    if (renderer === null || x === null)
      throw new Error("missing target camera");
    await expect(page.locator("html")).toHaveAttribute(
      "data-renderer",
      renderer,
    );
    await expect(page.locator("html")).toHaveAttribute("data-x", x);
    await expect(page.locator("html")).toHaveAttribute("data-zoom", "1");
    expect(await page.locator("html").getAttribute("data-instance")).toBe(
      scenario.loads === 1 ? "original" : null,
    );
  });
}
