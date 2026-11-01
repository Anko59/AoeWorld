//! Stone Wall and Gate. AoE II DE Feudal Age values (SiegeEngineers
//! aoe2techtree data.json): wall 5 stone, 1800 HP; gate 30 stone, 2750 HP.
use super::{BuildingRules, NO_UNITS, NONE, cost};
use crate::{Age, BuildingKind};

const fn barrier(
    kind: BuildingKind,
    stone: u32,
    seconds: u32,
    hp: u16,
    side: i32,
) -> BuildingRules {
    BuildingRules {
        kind,
        cost: cost(0, 0, 0, stone),
        build_seconds: seconds,
        max_hp: hp,
        side,
        depth: 1,
        los: if side == 1 { 2 } else { 6 },
        accepts: NONE,
        trainable: NO_UNITS,
        requires: None,
        buildable: true,
        blocks: true,
        food: 0,
        age: Age::Feudal,
    }
}

pub(super) const STONE_WALL: BuildingRules = barrier(BuildingKind::StoneWall, 5, 5, 1800, 1);
pub(super) const STONE_GATE: BuildingRules = barrier(BuildingKind::StoneGate, 30, 70, 2750, 4);
