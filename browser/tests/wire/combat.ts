import { readFileSync } from "node:fs";

// Shared with protocol/tests/browser_wire.rs: these fragments must match the
// current Rust codec whenever combat state changes. Values are hexadecimal.
// Unit tails contain hp, max_hp, attack_target and last_hit.
// These presentation fixtures start healthy, without a hit.
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
