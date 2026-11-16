//! The Fish Trap.
//! DAT FTRAP 199 (SiegeEngineers/aoe2techtree, DE update 188329): 100 wood,
//! 250 HP, 40 s, armour 1/1. A Fishing Ship builds and works it; it holds a
//! limited food amount and collapses once spent. The DAT gives no food
//! amount: 715 is the long-standing AoE II value (docs/navy.md).
use super::{BuildingRules, NO_UNITS, NONE, cost};
use crate::{Age, BuildingKind};

pub(super) const FISH_TRAP: BuildingRules = BuildingRules {
    kind: BuildingKind::FishTrap,
    cost: cost(0, 100, 0, 0),
    build_seconds: 40,
    max_hp: 250,
    side: 2,
    depth: 2,
    los: 1,
    population_support: 0,
    accepts: NONE,
    trainable: NO_UNITS,
    requires: None,
    buildable: true,
    blocks: false,
    food: 715,
    age: Age::Dark,
};
