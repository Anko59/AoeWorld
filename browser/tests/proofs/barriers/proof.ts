import { expect, type Page } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";
import { barrierArt } from "./art.js";
import { actors, prepareFort } from "./live.js";

type Clip = { item: string; start: number; end: number };
export async function barriersProof(
  page: Page,
  url: string,
  side: string,
  token: string,
  started: number,
) {
  expect(new URL(url).port).toBe("8191");
  const clips: Clip[] = [];
  const elapsed = () => (Date.now() - started) / 1000;
  const caption = async (text: string) =>
    page.evaluate((text) => {
      document.getElementById("barrier-caption")?.remove();
      const label = document.createElement("div");
      label.id = "barrier-caption";
      label.textContent = text;
      Object.assign(label.style, {
        position: "fixed",
        top: "60px",
        left: "20px",
        zIndex: "99999",
        color: "white",
        background: "#17130de6",
        font: "20px sans-serif",
        padding: "12px",
        pointerEvents: "none",
      });
      document.body.append(label);
    }, text);
  const screenshot = async (name: string) =>
    page.screenshot({ path: `../.cache/showcase/walls-${side}-${name}.png` });
  const live = await prepareFort(page, url, token);
  const arrived = async (x: number, y: number) => {
    await expect
      .poll(async () => {
        const unit = (await actors(page)).find((a) => a.id === live.worker);
        return unit ? Math.hypot(unit.x - x, unit.y - y) : Infinity;
      })
      .toBeLessThan(2);
  };
  await caption(
    `${side.toUpperCase()} · 2. Owner orders a move onto a distant closed gate`,
  );
  let start = elapsed();
  await screenshot("gate-first");
  await live.move(105, 118);
  for (let i = 0; i < 70; i++) {
    await live.step(1);
    await page.waitForTimeout(80);
    if (i === 35) await screenshot("gate-middle");
  }
  if (side === "after") await arrived(486.4, 379.2);
  else await arrived(1139.2, 475.2);
  await screenshot("gate-last");
  const observations: unknown[] = [
    { stage: "gate", actors: await actors(page) },
  ];
  if (side === "after") {
    await live.move(105, 114);
    for (let i = 0; i < 20; i++) {
      await live.step(1);
      await page.waitForTimeout(80);
    }
    await arrived(640, 302.4);
    await screenshot("gate-inside");
    observations.push({ stage: "inside", actors: await actors(page) });
    await caption(
      "AFTER · 2. Owner crosses the fort through its gate; it closes behind",
    );
    await live.move(116, 112);
    for (let i = 0; i < 40; i++) {
      await live.step(1);
      await page.waitForTimeout(80);
    }
    await arrived(1139.2, 475.2);
    await live.step(3);
    await screenshot("gate-crossed");
    observations.push({ stage: "crossed", actors: await actors(page) });
  }
  clips.push({ item: "gate", start, end: elapsed() });
  await writeFile(
    `../.cache/showcase/walls-${side}-clips.json`,
    JSON.stringify({ clips, observations }, null, 2),
  );
  const thaw = await barrierArt(page, url);
  await caption(
    `${side.toUpperCase()} · 1. Stone walls and stone/palisade gates · native art fixture`,
  );
  await page.waitForTimeout(2000);
  start = elapsed();
  await screenshot("art-first");
  await page.waitForTimeout(1500);
  await screenshot("art-middle");
  await page.waitForTimeout(1500);
  await screenshot("art-last");
  clips.push({ item: "art", start, end: elapsed() });
  thaw();
  await mkdir("../.cache/showcase", { recursive: true });
  await writeFile(
    `../.cache/showcase/walls-${side}-clips.json`,
    JSON.stringify({ clips, observations }, null, 2),
  );
}
