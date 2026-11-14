//! The Wonder: DAT 276 via aoe2techtree `data.json` (update 188329): 1000 wood,
//! 1000 stone, 1000 gold, 3500 s for one builder, 4800 hp, 5x5, LOS 8.
use super::{Age, BuildingKind, BuildingRules, NO_UNITS, NONE, cost};

pub(super) const WONDER: BuildingRules = BuildingRules {
    kind: BuildingKind::Wonder,
    cost: cost(0, 1000, 1000, 1000),
    build_seconds: 3500,
    max_hp: 4800,
    side: 5,
    depth: 5,
    los: 8,
    population_support: 0,
    accepts: NONE,
    trainable: NO_UNITS,
    requires: None,
    buildable: true,
    blocks: true,
    food: 0,
    age: Age::Imperial,
};
