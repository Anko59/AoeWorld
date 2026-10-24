import { readFile, readdir } from "node:fs/promises";
import type { CDPSession } from "@playwright/test";

export interface ProcessMemory {
  residentBytes: number;
  processes: { type: string; residentBytes: number }[];
}

// The exploration fixture launches a fresh Firefox with exactly one game tab.
// Reject absent/ambiguous content PIDs rather than passing with zero bytes or
// accidentally measuring a different tab, the parent, or an extension process.
export function firefoxTabMemory(tree: ProcessMemory): ProcessMemory {
  const processes = tree.processes.filter(({ type }) =>
    type.startsWith("Web Content:"),
  );
  const content = processes[0];
  if (processes.length !== 1 || !content)
    throw new Error(
      `Expected exactly one Firefox Web Content process, got ${processes.length}`,
    );
  return { residentBytes: content.residentBytes, processes };
}

// Playwright Firefox has no CDP process inventory. Its launched root and all
// descendants are visible in this container's /proc namespace instead.
export async function processTreeMemory(root: number): Promise<ProcessMemory> {
  // Parent PIDs include children launched by any browser thread; reading only
  // the main thread's task/children misses Firefox content processes.
  const inventory = new Map<
    number,
    { parent: number; residentBytes: number; name: string }
  >();
  for (const entry of await readdir("/proc")) {
    if (!/^\d+$/.test(entry)) continue;
    try {
      const status = await readFile(`/proc/${entry}/status`, "utf8");
      const parent = /^PPid:\s+(\d+)$/m.exec(status)?.[1];
      const rss = /^VmRSS:\s+(\d+)\s+kB$/m.exec(status)?.[1];
      if (parent && rss)
        inventory.set(Number(entry), {
          parent: Number(parent),
          residentBytes: Number(rss) * 1024,
          name: /^Name:\s+(.+)$/m.exec(status)?.[1] ?? "firefox",
        });
    } catch (error) {
      if (
        !["ENOENT", "ESRCH"].includes(
          (error as NodeJS.ErrnoException).code ?? "",
        )
      )
        throw error;
    }
  }
  const pending = [root];
  const seen = new Set<number>();
  const processes: ProcessMemory["processes"] = [];
  while (pending.length) {
    const pid = pending.pop();
    if (pid === undefined) break;
    if (seen.has(pid)) continue;
    seen.add(pid);
    const entry = inventory.get(pid);
    if (entry)
      processes.push({
        type: `${entry.name}:${pid}`,
        residentBytes: entry.residentBytes,
      });
    for (const [child, info] of inventory)
      if (info.parent === pid) pending.push(child);
  }
  if (!processes.length) throw new Error("Firefox process inventory is empty");
  return {
    residentBytes: processes.reduce(
      (sum, entry) => sum + entry.residentBytes,
      0,
    ),
    processes,
  };
}

// Chromium reports container-local PIDs. Summing VmRSS intentionally counts
// shared pages more than once: this is a conservative process-residency bound,
// not private memory, host-wide GPU residency, or hardware qualification.
export async function processMemory(
  session: CDPSession,
): Promise<ProcessMemory> {
  const result = (await session.send("SystemInfo.getProcessInfo")) as {
    processInfo: { id: number; type: string }[];
  };
  const processes = await Promise.all(
    result.processInfo.map(async ({ id, type }) => {
      const status = await readFile(`/proc/${id}/status`, "utf8");
      const value = /^VmRSS:\s+(\d+)\s+kB$/m.exec(status)?.[1];
      if (value === undefined)
        throw new Error(`Missing Chromium VmRSS for ${type} process ${id}`);
      return { type, residentBytes: Number(value) * 1024 };
    }),
  );
  if (!processes.some(({ type }) => type === "renderer"))
    throw new Error("Chromium memory sample has no renderer process");
  return {
    residentBytes: processes.reduce((sum, item) => sum + item.residentBytes, 0),
    processes,
  };
}
