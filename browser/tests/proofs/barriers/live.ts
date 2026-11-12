import { expect, type Page } from "@playwright/test";
import { uint } from "./art.js";

type Actor = {
  id: number;
  kind: string;
  hp: number;
  x: number;
  y: number;
  task: string;
};
export async function actors(page: Page): Promise<Actor[]> {
  return JSON.parse(
    (await page.locator("#scene").getAttribute("data-actors")) ?? "[]",
  ) as Actor[];
}

/** Ordinary gameplay messages and arena ticks; no authoritative state injection. */
export async function prepareFort(page: Page, url: string, token: string) {
  expect(
    (
      await page.request.post(`${url}maps/reset?scenario=feudal-economy`)
    ).status(),
  ).toBe(204);
  await page.addInitScript(() => {
    const Native = WebSocket;
    window.WebSocket = class extends Native {
      constructor(url: string | URL, protocols?: string | string[]) {
        super(url, protocols);
        if (String(url).includes("/game/ws"))
          Reflect.set(window, "barrierSocket", this);
      }
    };
  });
  await page.goto(`${url}?play=1#x=105&y=117&zoom=1&renderer=webgpu`);
  await expect(page.locator("#connection")).toHaveText("connected", {
    timeout: 60000,
  });
  const step = async (ticks: number, orders = 0) => {
    const response = await page.request.post(`${url}match/step`, {
      headers: { "x-aoeworld-admin-token": token },
      data: { ticks, orders },
    });
    expect(response.status()).toBe(200);
  };
  let sequence = 10000;
  const command = async (tag: number, rest: number[]) => {
    await page.evaluate(
      (bytes) => {
        const socket = Reflect.get(window, "barrierSocket") as WebSocket;
        socket.send(new Uint8Array(bytes));
      },
      [tag, ...uint(sequence++), ...rest],
    );
    await step(1, 1);
  };
  // Subscribe around the existing village to learn its real entity IDs.
  await page.evaluate(() => {
    location.hash = "x=130&y=130&zoom=.5&renderer=webgpu";
  });
  await expect
    .poll(async () => (await actors(page)).some((a) => a.kind === "TownCenter"))
    .toBe(true);
  const tc = (await actors(page)).find((a) => a.kind === "TownCenter");
  const worker = (await actors(page)).find((a) => a.kind === "Villager");
  if (!tc || !worker)
    throw new Error("The connected village has no town center or worker");
  await command(13, [...uint(tc.id), 2]);
  await step(200);
  await expect(page.locator("#age")).toContainText("Feudal Age");
  for (const [x, y] of [
    [100, 110],
    [110, 110],
    [110, 118],
    [100, 118],
  ] as const) {
    await command(2, [
      ...uint(worker.id),
      ...uint((x * 1024 + 512) * 2),
      ...uint((y * 1024 + 512) * 2),
    ]);
    await step(100);
  }
  const coords = (x: number, y: number) => [...uint(x * 2), ...uint(y * 2)];
  let queued = 0;
  for (const [x, y, a, b] of [
    [100, 110, 110, 110],
    [100, 111, 100, 117],
    [110, 111, 110, 117],
    [100, 118, 102, 118],
    [107, 118, 110, 118],
  ] as const) {
    await command(9, [
      1,
      ...uint(worker.id),
      21,
      ...coords(x, y),
      ...coords(a, b),
      queued,
    ]);
    queued = 1;
  }
  await command(8, [1, ...uint(worker.id), 22, ...coords(103, 118), 1]);
  await page.evaluate(() => {
    location.hash = "x=107&y=116&zoom=.6&renderer=webgpu";
  });
  for (let i = 0; i < 8; i++) {
    await step(200);
    if (
      (await actors(page)).some((a) => a.kind === "StoneGate" && a.hp === 2750)
    )
      break;
  }
  await expect
    .poll(
      async () =>
        (await actors(page)).filter(
          (a) => a.kind === "StoneWall" && a.hp === 1800,
        ).length,
    )
    .toBe(32);
  await expect
    .poll(async () =>
      (await actors(page)).some((a) => a.kind === "StoneGate" && a.hp === 2750),
    )
    .toBe(true);
  const move = (x: number, y: number) =>
    command(2, [
      ...uint(worker.id),
      ...uint((x * 1024 + 512) * 2),
      ...uint((y * 1024 + 512) * 2),
    ]);
  await move(116, 112);
  await step(100);
  await expect
    .poll(async () => {
      const unit = (await actors(page)).find((a) => a.id === worker.id);
      return (
        !!unit && unit.x > 20 && unit.x < 1260 && unit.y > 100 && unit.y < 515
      );
    })
    .toBe(true);
  const unit = (await actors(page)).find((a) => a.id === worker.id);
  if (!unit) throw new Error("The selected worker left the visible scene");
  await page.mouse.click(unit.x, unit.y - 10);
  await expect(page.locator("#selection")).toHaveAttribute(
    "data-kind",
    "Villager",
  );
  return { step, move, worker: worker.id };
}
