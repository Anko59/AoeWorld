import { expect, test, type Page } from "@playwright/test";
import { forceCanvas } from "../rendering/backends.js";
import { gameAssets } from "../game-assets.js";

type Actor = { id: number; kind: string; x: number; y: number };
async function actors(page: Page): Promise<Actor[]> {
  return JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as Actor[];
}
async function clickActor(page: Page, kind: string) {
  await expect
    .poll(async () => (await actors(page)).some((a) => a.kind === kind))
    .toBe(true);
  const actor = (await actors(page)).find((a) => a.kind === kind);
  const bounds = await page.locator("#scene").boundingBox();
  if (!actor || !bounds) throw new Error("Actor or canvas missing");
  await page.mouse.click(bounds.x + actor.x, bounds.y + actor.y);
}
async function start(page: Page) {
  expect((await page.request.post("/maps/reset")).status()).toBe(204);
  await gameAssets(page, true);
  await page.goto("/");
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect(page.locator("#population")).toHaveText("4/5");
  await page.keyboard.press("h");
  await page.keyboard.press("Escape");
}

test("stockpile and population come from the server; Q trains a villager", async ({
  page,
}) => {
  test.setTimeout(55_000);
  await start(page);
  for (const [resource, amount] of [
    ["wood", 200],
    ["food", 200],
    ["gold", 100],
    ["stone", 200],
  ] as const) {
    await expect(page.locator(`#stockpile-${resource}`)).toHaveText(
      String(amount),
    );
  }
  await expect(page.locator("#population")).toHaveText("4/5");
  await clickActor(page, "TownCenter");
  await expect(page.locator("#selection-name")).toHaveText("Town Center");
  await expect(
    page.getByRole("button", { name: /Train Villager/ }),
  ).toContainText("50");
  await page.keyboard.press("q");
  await expect(page.locator("#stockpile-food")).toHaveText("150");
  await expect(page.locator("#training-progress")).toBeVisible();
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(page.locator("#stockpile-food")).toHaveText("200");
  await expect(page.locator("#training-progress")).toBeHidden();
  await page.keyboard.press("q");
  await expect(page.locator("#stockpile-food")).toHaveText("150");
  await expect(page.locator("#population")).toHaveText("5/5", {
    timeout: 32_000,
  });
  await page.keyboard.press("q");
  await expect(page.locator("#connection")).toContainText("population limit");
});

test("right clicking wood sends villagers to gather and deposit", async ({
  page,
}) => {
  test.setTimeout(100_000);
  await start(page);
  const hash = process.env["AOE_E2E_LANDSCAPE_HASH"];
  if (!hash) throw new Error("Run through the Dockerized browser gate");
  const token = await page.evaluate(() =>
    sessionStorage.getItem("aoeworld.resume-token"),
  );
  if (!token) throw new Error("Controller token missing");
  const opened = await page.request.post(`/maps/${hash}`, {
    headers: { "X-AoeWorld-Controller-Token": token },
  });
  expect(opened.ok()).toBeTruthy();
  await page.getByRole("button", { name: "Menu", exact: true }).click();
  await page.getByRole("button", { name: "Reconnect to server" }).click();
  await expect(page.locator("#minimap")).toHaveAttribute("data-map-hash", hash);
  await expect(page.locator("#connection")).toHaveText("connected");
  await page.keyboard.press("h");
  await page.keyboard.press("Escape");
  await clickActor(page, "Villager");
  await page.mouse.dblclick(
    ...(await page.locator("#scene").evaluate((canvas): [number, number] => {
      const a = (
        JSON.parse(canvas.getAttribute("data-actors") ?? "[]") as Actor[]
      ).find((v) => v.kind === "Villager");
      if (!a) throw new Error("Villager missing");
      const r = canvas.getBoundingClientRect();
      return [r.x + a.x, r.y + a.y];
    })),
  );
  await expect(page.locator("#selection")).toHaveAttribute("data-count", "3");
  const woodTarget = () =>
    page.locator("#scene").evaluate((canvas) => {
      const nodes = JSON.parse(
        canvas.getAttribute("data-resource-targets") ?? "[]",
      ) as { kind: number; x: number; y: number; distance: number }[];
      return nodes
        .filter((n) => n.kind === 1)
        .sort((a, b) => a.distance - b.distance)[0];
    });
  await expect.poll(woodTarget).toBeDefined();
  const resource = await woodTarget();
  const box = await page.locator("#scene").boundingBox();
  if (!box || !resource) throw new Error("Resource or canvas missing");
  await page.mouse.click(box.x + resource.x, box.y + resource.y, {
    button: "right",
  });
  await expect(page.locator("#workers-wood")).toHaveText("3", {
    timeout: 35_000,
  });
  await expect
    .poll(
      async () => Number(await page.locator("#stockpile-wood").innerText()),
      { timeout: 60_000 },
    )
    .toBeGreaterThan(200);
});

test("menu resumes and quits; minimap lives in the bottom frame and navigates", async ({
  page,
}) => {
  await start(page);
  await page.getByRole("button", { name: "Menu", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "Game menu" })).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Settings", exact: true }),
  ).toBeDisabled();
  await page.getByRole("button", { name: "Resume", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "Game menu" })).toBeHidden();
  await expect(page.locator("#bottom-panel #minimap")).toBeVisible();
  const before = await page.locator("#minimap").getAttribute("data-camera-x");
  await page.locator("#minimap-map").click({ position: { x: 30, y: 30 } });
  await expect(page.locator("#minimap")).not.toHaveAttribute(
    "data-camera-x",
    before ?? "",
  );
  await page.getByRole("button", { name: "Menu", exact: true }).click();
  await page.getByRole("button", { name: "Quit to Main Menu" }).click();
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect(page.getByRole("dialog", { name: "Game menu" })).toBeHidden();
});

test("Canvas replacement keeps village selection, drag feedback and training controls", async ({
  page,
}) => {
  await forceCanvas(page);
  await start(page);
  await expect(page.locator("#playground")).toHaveAttribute(
    "data-renderer",
    "canvas2d",
  );
  await expect(page.locator("#scene")).toHaveAttribute(
    "data-village-art",
    "markers",
  );
  await page.mouse.move(300, 220);
  await page.mouse.down();
  await page.mouse.move(420, 280, { steps: 3 });
  await expect(page.locator("#drag-box")).toBeVisible();
  await page.mouse.up();
  await expect(page.locator("#drag-box")).toBeHidden();
  await clickActor(page, "Villager");
  await expect(page.locator("#selection-name")).toHaveText("Villager");
  await clickActor(page, "TownCenter");
  await page.getByRole("button", { name: /Train Villager/ }).click();
  await expect(page.locator("#stockpile-food")).toHaveText("150");
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(page.locator("#stockpile-food")).toHaveText("200");
});
