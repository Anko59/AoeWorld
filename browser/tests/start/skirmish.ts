import { expect, type Page, type Route } from "@playwright/test";

// Scripted skirmish endpoints for the start specs. The E2E server has no AI
// host, so phases are served here; the map, its activation and the terrain
// wait stay real.
export type Status = Record<string, unknown>;

export async function setup(page: Page) {
  await page.goto("/?skipIntro=1");
  await page
    .getByRole("button", { name: "Single Player", exact: true })
    .click();
  await expect(page.getByRole("heading", { name: "Game Setup" })).toBeVisible();
  await expect(page.locator("#connection")).toHaveText("connected");
}

/** A completed map job for the prepared fixture map. */
export async function completedJob(page: Page, hash: string) {
  await page.route("**/maps/jobs", async (route) => {
    if (route.request().method() === "GET") return route.fulfill({ json: [] });
    await route.fulfill({ json: { id: 71 } });
  });
  await page.route("**/maps/jobs/71", (route) =>
    route.fulfill({
      json: {
        id: 71,
        state: "completed",
        content_hash: hash,
        stage: "completed",
      },
    }),
  );
}

/** Scripted skirmish endpoints; `phases` are served one per status poll. */
export async function skirmish(page: Page, phases: Status[]) {
  const posted: Status[] = [];
  const stops: string[] = [];
  let index = -1;
  const current = () => ({
    id: 5,
    ai_available: true,
    max_opponents: 99,
    opponents: 3,
    ai_hosts: 1,
    ...phases[Math.max(0, Math.min(index, phases.length - 1))],
  });
  let onPost: (route: Route) => Promise<void> = async () => {};
  await page.route("**/skirmish", async (route) => {
    if (route.request().method() === "POST") {
      posted.push(route.request().postDataJSON());
      await onPost(route);
      index = 0;
      return route.fulfill({ status: 202, json: current() });
    }
    if (index < 0)
      return route.fulfill({ json: { ...current(), id: 4, phase: "idle" } });
    index += 1;
    await route.fulfill({ json: current() });
  });
  await page.route("**/skirmish/stop", async (route) => {
    stops.push(route.request().headers()["x-aoeworld-controller-token"] ?? "");
    phases.push({ phase: "cancelled" });
    index = phases.length - 1;
    await route.fulfill({ json: current() });
  });
  return {
    posted,
    stops,
    setOnPost: (handler: typeof onPost) => (onPost = handler),
  };
}
