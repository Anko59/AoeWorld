import { readFileSync } from "node:fs";

// Shared with protocol/tests/browser_wire.rs: one owned sheep near the village.
const sheep = readFileSync(new URL("sheep.hex", import.meta.url), "utf8")
  .trim()
  .split(/\s+/)
  .map((byte) => Number.parseInt(byte, 16));

export function wildlifeState(revision: number[]): Buffer {
  // WildlifeState, subscription revision, tick, count, animal state.
  return Buffer.from([8, ...revision, 0, 1, ...sheep]);
}
