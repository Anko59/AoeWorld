import { expect, type Page } from "@playwright/test";
import { readFile, writeFile } from "node:fs/promises";
import { syntheticChunk } from "../synthetic-map.js";
import { disabledFog } from "../wire/fog.js";
import { marketPrices } from "../wire/buildings.js";
import { position, sint, uint } from "../village/input/fixture.js";
import {
  memorySnapshot,
  type MemorySnapshot,
} from "../memory/exploration-fixture.js";
import { processMemory, type ProcessMemory } from "../memory/process.js";

/** Local renderer workload: original artwork, synthetic authoritative snapshots.
 * This measures the real client, not a complete RTS match or hardware fitness. */
export async function pressureProof(page: Page, url: string, phase: string) {
  const browser = page.context().browser();
  if (!browser)
    throw new Error("Pressure measurement requires a browser process");
  const session = await browser.newBrowserCDPSession();
  expect((await page.request.get(`${url}asset-pack/manifest.json`)).ok()).toBe(
    true,
  );
  const unit = (i: number) => [
    ...uint(i),
    ...uint(Math.floor(i / 200)),
    ...position(
      8192.5 + ((i % 50) - 24.5) * 0.24,
      8192.5 + (Math.floor(i / 50) - 19.5) * 0.24,
    ),
    1,
    0,
    Math.floor(i / 24) % 8,
    i % 24,
    0,
    0,
    0,
    ...uint(100),
    ...uint(100),
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
  ];
  const sides = [
    4, 2, 2, 2, 2, 3, 3, 1, 1, 3, 3, 3, 3, 4, 1, 4, 4, 3, 4, 4, 1, 1, 4,
  ];
  const buildings: number[][] = [];
  for (let state = 0; state < 3; state++)
    for (let kind = 0; kind < 23; kind++) {
      if (kind === 20) continue; // Bridges have a separate endpoint contract.
      const n = buildings.length;
      const side = sides[kind];
      if (side === undefined)
        throw new Error("Missing pressure building footprint");
      const depth = kind === 15 || kind === 22 ? 1 : side;
      const x = 8183 + (n % 11) * 1.6,
        y = 8183 + Math.floor(n / 11) * 2.5;
      buildings.push([
        ...uint(3000 + n),
        n % 10,
        kind,
        ...sint(Math.round(x)),
        ...sint(Math.round(y)),
        ...sint(Math.round(x) + side),
        ...sint(Math.round(y) + depth),
        1,
        ...uint(1000),
        ...uint(1000 - state * 300),
        ...uint(1000),
        8,
        ...(kind === 5 ? [1, ...uint(375)] : [0]),
        0,
        0,
        0,
        2,
        0,
        0,
        0,
        0,
        0,
      ]);
    }
  const units = Array.from({ length: 2000 }, (_, i) => unit(i)).flat();
  const players = Array.from({ length: 10 }, (_, i) => [
    i,
    ...uint(2000),
    ...uint(2000),
    ...uint(2000),
    ...uint(2000),
    ...uint(200),
    ...uint(200),
    2,
    ...uint(4 + 16384),
    ...marketPrices,
  ]).flat();
  const fixtureChunks = [-1, 0].flatMap((y) =>
    [-1, 0].map((x) => syntheticChunk(256 + x, 256 + y, false)),
  );
  await showcasePage({
    hello: [
      0,
      2,
      1,
      0,
      0,
      ...uint(32768),
      ...uint(32768),
      ...uint(1024),
      20,
      0,
      0,
      0,
    ],
    snapshot: [
      ...uint(2000),
      ...units,
      10,
      ...players,
      ...uint(buildings.length),
      ...buildings.flat(),
    ],
    chunks: fixtureChunks,
  });
  if (phase === "pressure-export") {
    await session.detach();
    return;
  }
  const timers: ReturnType<typeof setInterval>[] = [];
  await page.routeWebSocket("**/game/ws", (socket) => {
    let tick = 0;
    let started = false;
    let revision: number[] = [];
    socket.onMessage((message) => {
      const bytes =
        typeof message === "string" ? Buffer.from(message) : message;
      if (bytes[0] === 0)
        socket.send(
          Buffer.from([
            0,
            2,
            1,
            0,
            0,
            ...uint(32768),
            ...uint(32768),
            ...uint(1024),
            20,
            0,
            0,
            0,
          ]),
        );
      if (bytes[0] !== 1) return;
      revision = [];
      for (const byte of bytes.subarray(1)) {
        revision.push(byte);
        if (byte < 128) break;
      }
      const snapshot = () => {
        socket.send(
          Buffer.from([
            1,
            ...revision,
            ...uint(tick),
            ...uint(2000),
            ...units,
            10,
            ...players,
            ...uint(buildings.length),
            ...buildings.flat(),
          ]),
        );
        tick += 2;
      };
      snapshot();
      socket.send(disabledFog(revision));
      if (!started) timers.push(setInterval(snapshot, 100));
      started = true;
    });
  });
  try {
    await page.goto(
      `${url}?play=1&perf=1#x=8192.5&y=8192.5&zoom=.35&renderer=webgpu`,
    );
    await expect(page.locator("#connection")).toHaveText("connected", {
      timeout: 90000,
    });
    await expect(page.locator("#playground")).toHaveAttribute(
      "data-renderer",
      "webgpu",
    );
    await page.evaluate(async (payload) => {
      const game = await import(
        new URL("/pkg/aoe_client.js", location.href).href
      );
      game.activate_surface_fixture("06".repeat(32), payload);
      const label = document.createElement("div");
      label.textContent =
        "Synthetic pressure scene · 2,000 mixed units · original local DE art";
      label.style.cssText =
        "position:fixed;top:55px;left:10px;background:#000b;color:white;padding:8px;z-index:9999";
      document.body.append(label);
    }, JSON.stringify(fixtureChunks));
    const samples: {
      zoom: number;
      memory: MemorySnapshot;
      process: ProcessMemory;
      performance: unknown;
      scene: Record<string, string | undefined>;
      strategic?: Record<string, string | undefined>;
    }[] = [];
    const repaired =
      phase === "pressure-after" || phase === "pressure-extremes";
    const zooms =
      phase === "pressure-extremes" ? [0.1, 1 / 4096, 0.25] : [0.35, 0.25];
    for (const zoom of zooms) {
      const mapView = zoom === 1 / 4096;
      const firstFrame = Number(
        await page.locator("#scene").getAttribute("data-frame"),
      );
      const clock = await page.locator("#game-clock").textContent();
      await page.evaluate((z) => {
        location.hash = `x=8192.5&y=8192.5&zoom=${z}&renderer=webgpu`;
      }, zoom);
      if (repaired && !mapView) {
        await expect
          .poll(
            async () =>
              Number(await page.locator("#scene").getAttribute("data-frame")),
            { timeout: 60000 },
          )
          .toBeGreaterThan(firstFrame);
      }
      if (repaired)
        await expect(page.locator("#scene")).toHaveAttribute(
          "data-lod",
          mapView ? "map" : "sprites",
        );
      if (mapView) {
        await expect(page.locator("#strategic-scene")).toBeVisible();
        await expect(page.locator("#strategic-label")).toContainText("0.0244%");
        await expect
          .poll(async () => {
            const picks = JSON.parse(
              (await page
                .locator("#strategic-scene")
                .getAttribute("data-picks")) ?? "[]",
            ) as { count: number }[];
            return picks.reduce((sum, p) => sum + p.count, 0);
          })
          .toBe(2000 + buildings.length);
      }
      for (let n = 0; n < 12; n++) {
        await page.waitForTimeout(1000);
        const memory = await memorySnapshot(page);
        expect(memory.owned.units).toBe(2000);
        const processMetrics = await processMemory(session);
        if (repaired) {
          expect(memory.owned.backend).toBe("webgpu");
          expect(memory.owned.native_failed).toBe(0);
          expect(memory.wasmBytes).toBeLessThanOrEqual(512 * 1024 ** 2);
          expect(
            Object.fromEntries(memory.owned.renderer).gpu_texture_bytes,
          ).toBeLessThanOrEqual(520 * 1024 ** 2);
          // Shared pages count multiple times: a conservative local process
          // bound, not private GPU memory or hardware qualification.
          expect(processMetrics.residentBytes).toBeLessThan(4 * 1024 ** 3);
        }
        samples.push({
          zoom,
          memory,
          process: processMetrics,
          performance: await page.evaluate(() =>
            Reflect.get(window, "aoePerformance"),
          ),
          strategic: mapView
            ? await page
                .locator("#strategic-scene")
                .evaluate((e) => ({ ...(e as HTMLElement).dataset }))
            : undefined,
          scene: await page
            .locator("#scene")
            .evaluate((e) => ({ ...(e as HTMLElement).dataset })),
        });
      }
      await expect(page.locator("#game-clock")).not.toHaveText(clock ?? "");
      if (repaired && !mapView) {
        expect(
          Number(await page.locator("#scene").getAttribute("data-frame")),
        ).toBeGreaterThan(firstFrame + 2);
      }
      if (repaired && !mapView)
        await expect(page.locator("#strategic-scene")).toBeHidden();
      await page.screenshot({ path: `../.cache/ui/${phase}-${zoom}.png` });
    }
    await writeFile(
      `../.cache/ui/${phase}.json`,
      JSON.stringify(
        {
          scope:
            "real client/original art; synthetic snapshots, not RTS or hardware qualification",
          units: 2000,
          kinds: 24,
          buildings: buildings.length,
          samples,
        },
        null,
        2,
      ),
    );
  } finally {
    for (const timer of timers) clearInterval(timer);
    await session.detach();
  }
}

/** Ignored local page for make showcase: the same real client and frozen wire
 * fixture, without changing production routes or embedding original artwork. */
async function showcasePage(data: {
  hello: number[];
  snapshot: number[];
  chunks: ReturnType<typeof syntheticChunk>[];
}) {
  const setup = `<script>
const pressure = ${JSON.stringify(data)};
const uint = ${uint.toString()};
location.hash = 'x=8192.5&y=8192.5&zoom=.35&renderer=webgpu';
const OriginalSocket = window.WebSocket, sockets = [];
class PressureSocket extends EventTarget {
  static CONNECTING=0; static OPEN=1; static CLOSING=2; static CLOSED=3;
  constructor(url) {
    super();
    if (!String(url).endsWith('/game/ws')) return new OriginalSocket(url);
    this.url=String(url); this.readyState=0; this.bufferedAmount=0;
    sockets.push(this);
    setTimeout(()=>{ this.readyState=1; this.emit('open'); },0);
  }
  emit(kind, bytes) {
    const event=kind==='message'
      ? new MessageEvent(kind,{data:Uint8Array.from(bytes).buffer})
      : new Event(kind);
    this.dispatchEvent(event); this['on'+kind]?.(event);
  }
  send(data) {
    const bytes=data instanceof ArrayBuffer ? new Uint8Array(data)
      : new Uint8Array(data.buffer,data.byteOffset,data.byteLength);
    const copy=bytes.slice();
    setTimeout(()=>this.respond(copy),0);
  }
  respond(bytes) {
    if (bytes[0]===0) this.emit('message',pressure.hello);
    if (bytes[0]!==1) return;
    this.revision=[];
    for (const byte of bytes.subarray(1)) { this.revision.push(byte); if(byte<128)break; }
    if(!this.snapshot) {
      this.tick=0;
      this.snapshot=()=>{ this.emit('message',[1,...this.revision,...uint(this.tick),...pressure.snapshot]); this.tick+=2; };
    }
    this.snapshot(); this.emit('message',[9,...this.revision,0,0,0,2,2,1,0,0]);
    if(!this.timer)this.timer=setInterval(this.snapshot,100);
  }
  close() { clearInterval(this.timer); this.readyState=3; setTimeout(()=>this.emit('close'),0); }
}
window.WebSocket=PressureSocket;
window.addEventListener('pagehide',()=>sockets.forEach(socket=>socket.close()));
(async()=>{
  const pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
  for(let n=0;n<2400;n++) {
    if(document.querySelector('#connection')?.textContent==='connected')break;
    await pause(50);
  }
  const game=await import('/pkg/aoe_client.js');
  game.activate_surface_fixture('06'.repeat(32),JSON.stringify(pressure.chunks));
  const label=document.createElement('div');
  label.textContent='Synthetic pressure scene · 2,000 mixed units · original local DE art';
  label.style.cssText='position:fixed;top:55px;left:10px;background:#000b;color:white;padding:8px;z-index:9999';
  document.body.append(label);
  const frame=Number(document.querySelector('#scene').dataset.frame||0);
  await pause(12000);
  for(let n=0;n<1200;n++) {
    if(Number(document.querySelector('#scene').dataset.frame||0)>=frame+2) {
      label.dataset.pressureReady='true'; return;
    }
    await pause(50);
  }
  throw Error('Pressure scene never presented two frames');
})();
</script>`;
  const html = (await readFile("../web/index.html", "utf8")).replace(
    "</head>",
    `${setup}</head>`,
  );
  await writeFile("../.cache/ui/pressure-proof.html", html);
  await writeFile("../web/pkg/pressure-proof.html", html);
}
