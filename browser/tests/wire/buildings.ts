import { readFileSync } from "node:fs";

// Shared with protocol/tests/browser_wire.rs to pin the current Rust layout.
function fragment(name: string): number[] {
  return readFileSync(new URL(name, import.meta.url), "utf8")
    .trim()
    .split(/\s+/)
    .map((byte) => Number.parseInt(byte, 16));
}

// After footprint: construction, progress, hp, max_hp, los, food_remaining,
// training_queue, research, last_hit (complete, healthy and idle).
export const townCenterState = fragment("town-center.hex");
// After population_cap: age (Dark), researched (empty bitset).
export const darkAgeState = fragment("player.hex");
export const dockState = fragment("dock.hex");
export const fishingShipState = fragment("fishing-ship.hex");
