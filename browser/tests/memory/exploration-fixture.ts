import type { Page, WebSocketRoute } from "@playwright/test";
import { gameAssets } from "../game-assets.js";
import { syntheticChunk } from "../synthetic-map.js";
import { disabledFog } from "../wire/fog.js";

const hash = "02".repeat(32);

function uint(value: number): number[] {
  const bytes: number[] = [];
  do {
    const digit = value % 128;
    value = Math.floor(value / 128);
    bytes.push(digit + (value ? 128 : 0));
  } while (value);
  return bytes;
}

/** Real fetch/decode/eviction/render lifecycle on a synthetic 2,048-square map.
 * No per-camera fixture reset: previously visited chunks must actually evict. */
export async function explorationFixture(
  page: Page,
  onSocket?: (socket: WebSocketRoute) => void,
) {
  await gameAssets(page, true);
  await page.routeWebSocket("**/game/ws", (socket) => {
    onSocket?.(socket);
    socket.onMessage((message) => {
      const bytes =
        typeof message === "string" ? Buffer.from(message) : message;
      if (bytes[0] === 0) {
        socket.send(
          Buffer.from([
            0,
            4,
            1,
            1,
            ...Array<number>(32).fill(2),
            0,
            ...uint(4096),
            ...uint(4096),
            ...uint(1024),
            20,
            1,
            ...Array<number>(24).fill(1),
          ]),
        );
        return;
      }
      if (bytes[0] !== 1) return;
      const revision: number[] = [];
      for (let index = 1; index < bytes.length; index++) {
        const byte = bytes[index] ?? 0;
        revision.push(byte);
        if (byte < 128) break;
      }
      socket.send(Buffer.from([1, ...revision, 0, 0, 0, 0]));
      socket.send(disabledFog(revision));
      // Empty authoritative depletion reset: all fixture trees start intact.
      socket.send(Buffer.from([7, ...revision, 0, 0, 0]));
    });
  });
  await page.route(`**/maps/${hash}/height-bounds`, (route) =>
    route.fulfill({
      json: {
        content_hash: hash,
        minimum_height_level: 0,
        maximum_height_level: 0,
      },
    }),
  );
  const minimap = Buffer.alloc(8 + 512 ** 2, 0x80);
  minimap.write("AWMM");
  minimap.writeUInt16LE(512, 4);
  minimap.writeUInt16LE(0, 6);
  await page.route(`**/maps/${hash}/minimap`, (route) =>
    route.fulfill({ body: minimap, contentType: "application/octet-stream" }),
  );
  const visits = new Map<string, number>();
  const payloads = new Map<string, string>();
  await page.route(`**/maps/${hash}/chunks/*/*`, async (route) => {
    const [x = 0, y = 0] = route
      .request()
      .url()
      .split("/")
      .slice(-2)
      .map(Number);
    const key = `${x},${y}`;
    visits.set(key, (visits.get(key) ?? 0) + 1);
    // Sparse trees retain real resource rendering without software-raster cost
    // dominating this residency regression. The dedicated forest test is dense.
    // Reuse deterministic fixture bytes on the Node side; the browser still
    // performs every real fetch/decode/refetch through its unchanged cache.
    let body = payloads.get(key);
    if (body === undefined) {
      body = JSON.stringify(
        syntheticChunk(x, y, (tx, ty) => tx % 32 === 0 && ty % 32 === 0),
      );
      payloads.set(key, body);
    }
    await route.fulfill({ body, contentType: "application/json" });
  });
  return visits;
}

export interface MemorySnapshot {
  wasmBytes: number;
  backingPixels: number;
  historyEntries: number;
  jsHeapBytes: number | null;
  chromiumDiagnostics?: { heap: unknown; native: unknown };
  minimapRgbaBytes: number;
  fogCanvasBytes: number | null;
  fogWorkingBytes: number | null;
  owned: {
    backend: string;
    client_references: number;
    terrain_chunks: number;
    terrain_bytes: number;
    terrain_requests: number;
    height_probes: number;
    height_candidates: number;
    fog_bytes: number;
    scene_bytes: number;
    render_frame_bytes: number;
    units: number;
    unit_bytes: number;
    interpolation_bytes: number;
    buildings_and_ghosts: number;
    animals: number;
    projectiles: number;
    resource_entries: number;
    resource_bytes: number;
    stump_effects: number;
    outgoing_message_bytes: number;
    renderer: [string, number][];
  };
}

export async function memorySnapshot(page: Page): Promise<MemorySnapshot> {
  return page.evaluate(async () => {
    const module = (await import(
      new URL("/pkg/aoe_client.js", location.href).href
    )) as {
      default(): Promise<{ memory: WebAssembly.Memory }>;
      client_memory_snapshot(): string;
    };
    return {
      wasmBytes: (await module.default()).memory.buffer.byteLength,
      backingPixels: (() => {
        const canvas = document.querySelector<HTMLCanvasElement>("#scene");
        if (!canvas) throw new Error("Missing game canvas");
        return canvas.width * canvas.height;
      })(),
      historyEntries: history.length,
      // Logical RGBA raster payload; process RSS also includes browser decoding
      // and allocator overhead. The diagnostic DOM value is the raster axis.
      minimapRgbaBytes:
        4 *
        Number(
          document.querySelector<HTMLElement>("#minimap")?.dataset
            .rasterSamples ?? 0,
        ) **
          2,
      fogCanvasBytes: (() => {
        const canvas = document.querySelector<HTMLCanvasElement>(
          "#minimap-fog canvas",
        );
        return canvas ? canvas.width * canvas.height * 4 : null;
      })(),
      fogWorkingBytes: (() => {
        const bytes = document
          .querySelector("#minimap-fog")
          ?.getAttribute("data-working-bytes");
        return bytes === null || bytes === undefined ? null : Number(bytes);
      })(),
      jsHeapBytes:
        (performance as Performance & { memory?: { usedJSHeapSize: number } })
          .memory?.usedJSHeapSize ?? null,
      owned: JSON.parse(
        module.client_memory_snapshot(),
      ) as MemorySnapshot["owned"],
    };
  });
}
