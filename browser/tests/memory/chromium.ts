// Chromium retains inspector response bodies separately from the application.
// Bound that diagnostic history while preserving every application request,
// process RSS sample, renderer flag and memory assertion. The public CDP
// transport is needed because Playwright enables its own Network sessions.
import type {
  Browser,
  BrowserType,
  ConnectOverCDPTransport,
} from "@playwright/test";
import { spawn } from "node:child_process";
import type { Readable, Writable } from "node:stream";
import { access, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

export const INSPECTOR_HISTORY_BYTES = 1024 ** 2;
export const INSPECTOR_RESOURCE_BYTES = 128 * 1024;

export async function launchMemoryBrowser(
  chromium: BrowserType,
  args: string[],
) {
  // Match Playwright's normal headless executable in the pinned Linux image;
  // full Chrome also starts UI/extension renderers unrelated to this workload.
  const executable = chromium
    .executablePath()
    .replace(
      /\/chromium-(\d+)\/chrome-linux64\/chrome$/,
      "/chromium_headless_shell-$1/chrome-headless-shell-linux64/chrome-headless-shell",
    );
  if (executable === chromium.executablePath())
    throw new Error("Expected the pinned Linux Chromium layout");
  await access(executable);
  const profile = await mkdtemp(join(tmpdir(), "aoeworld-rss-"));
  const child = spawn(
    executable,
    [
      "--headless",
      "--no-sandbox",
      "--disable-dev-shm-usage",
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-background-networking",
      "--disable-extensions",
      "--disable-component-extensions-with-background-pages",
      "--disable-default-apps",
      "--disable-sync",
      "--disable-background-timer-throttling",
      "--disable-renderer-backgrounding",
      "--disable-backgrounding-occluded-windows",
      "--hide-scrollbars",
      "--mute-audio",
      "--force-color-profile=srgb",
      "--remote-debugging-pipe",
      "--no-startup-window",
      `--user-data-dir=${profile}`,
      ...args,
    ],
    { stdio: ["ignore", "ignore", "ignore", "pipe", "pipe"] },
  );
  let browser: Browser | undefined;
  const stop = async () => {
    if (!child.pid || child.exitCode !== null || child.signalCode !== null)
      return;
    const exited = new Promise<void>((resolve) =>
      child.once("exit", () => resolve()),
    );
    const timer = setTimeout(() => child.kill("SIGKILL"), 5_000);
    child.kill("SIGTERM");
    try {
      await exited;
    } finally {
      clearTimeout(timer);
    }
  };
  try {
    const input = child.stdio[3] as Writable;
    const output = child.stdio[4] as Readable;
    let pending = Buffer.alloc(0);
    let closed = false;
    const disconnected = (reason: string) => {
      if (closed) return;
      closed = true;
      transport.onclose?.(reason);
    };
    const transport: ConnectOverCDPTransport = {
      send(raw) {
        if (closed) throw new Error("CDP pipe is closed");
        const message = raw as {
          method: string;
          params?: Record<string, unknown>;
        };
        if (message.method === "Network.enable") {
          message.params = {
            ...message.params,
            maxTotalBufferSize: INSPECTOR_HISTORY_BYTES,
            maxResourceBufferSize: INSPECTOR_RESOURCE_BYTES,
          };
        }
        input.write(JSON.stringify(message) + "\0");
      },
      close() {
        input.end();
        disconnected("CDP pipe closed");
      },
    };
    output.on("data", (chunk: Buffer) => {
      pending = Buffer.concat([pending, chunk]);
      for (let end = pending.indexOf(0); end !== -1; end = pending.indexOf(0)) {
        const value = JSON.parse(pending.subarray(0, end).toString()) as object;
        pending = pending.subarray(end + 1);
        transport.onmessage?.(value);
      }
    });
    output.once("end", () => disconnected("CDP pipe ended"));
    output.once("error", () => disconnected("CDP pipe failed"));
    input.once("error", () => disconnected("CDP pipe failed"));
    child.once("exit", () => disconnected("Chromium exited"));
    child.once("error", () => disconnected("Chromium launch failed"));
    browser = await chromium.connectOverCDP(transport, { timeout: 10_000 });
    return {
      browser,
      async close() {
        try {
          await stop();
          await browser?.close();
        } finally {
          await stop();
          await rm(profile, { recursive: true, force: true });
        }
      },
    };
  } catch (error) {
    await stop();
    await rm(profile, { recursive: true, force: true });
    throw error;
  }
}
