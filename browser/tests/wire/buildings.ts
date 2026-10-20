import { readFileSync } from "node:fs";

// Shared with protocol/tests/browser_wire.rs to pin the current Rust layout.
// Town Center fields after footprint: construction, progress_permille, hp,
// max_hp, los, food_remaining, training_queue, last_hit (complete, healthy and idle).
export const townCenterState = readFileSync(
  new URL("town-center.hex", import.meta.url),
  "utf8",
)
  .trim()
  .split(/\s+/)
  .map((byte) => Number.parseInt(byte, 16));
