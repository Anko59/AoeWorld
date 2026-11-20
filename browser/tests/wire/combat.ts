import { readFileSync } from "node:fs";

// Shared with protocol/tests/browser_wire.rs: these fragments must match the
// current Rust codec whenever combat state changes. Values are hexadecimal.
// Unit tails contain hp, max_hp, attack_target and last_hit.
// Current tails include auto-scout and the authoritative combat stance.
// These presentation fixtures start healthy, without a hit or auto-scout.
function fragment(name: string): number[] {
  return readFileSync(new URL(`${name}.hex`, import.meta.url), "utf8")
    .trim()
    .split(/\s+/)
    .filter(Boolean)
    .map((byte) => Number.parseInt(byte, 16));
}

export const villagerCombat = fragment("villager");
export const scoutCombat = fragment("scout");

export const militiaCombat = fragment("militia");

// Keep cargo in codec-checked tails so new fields cannot shift its position.
const transportTails = {
  0: fragment("transport-ship-empty"),
  2: fragment("transport-ship-loaded"),
} as const;

export const transportCombat = (cargo: keyof typeof transportTails) =>
  transportTails[cargo];
