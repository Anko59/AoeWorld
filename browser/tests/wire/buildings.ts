import { readFileSync } from "node:fs";

// Shared with protocol/tests/browser_wire.rs to pin the current Rust layout.
function fragment(name: string): number[] {
  return readFileSync(new URL(name, import.meta.url), "utf8")
    .trim()
    .split(/\s+/)
    .map((byte) => Number.parseInt(byte, 16));
}

// After footprint: construction, progress, hp, max_hp, los, food_remaining,
// training_queue, research, last_hit, observed age, gate_open, rally_point, prepaid farm_reseeds, bounded garrison list, optional bridge endpoints (complete, healthy and idle).
export const townCenterState = fragment("town-center.hex");
// After population: age (Dark), researched (empty bitset).
export const darkAgeState = fragment("player.hex");
// After researched: the three Market prices (wood, food, stone), 100 gold each.
export const marketPrices = fragment("market-prices.hex");
export const dockState = fragment("dock.hex");
export const fishingShipState = fragment("fishing-ship.hex");
// Feudal Age researched, every other technology still available.
export const feudalAgeState = fragment("feudal-player.hex");

// Feudal Age and Wheelbarrow researched; carry capacity is 13.
export const wheelbarrowAgeState = fragment("wheelbarrow-player.hex");
