import { readFile } from "node:fs/promises";
import type { CDPSession } from "@playwright/test";

export interface ProcessMemory {
  residentBytes: number;
  processes: { type: string; residentBytes: number }[];
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
