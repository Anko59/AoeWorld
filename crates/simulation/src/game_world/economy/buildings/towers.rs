//! DE DAT tower line (unit records 79, 234, 235).
use super::{Age, BuildingKind, BuildingRules, NO_UNITS, NONE, cost};

pub(super) const WATCH_TOWER: BuildingRules = BuildingRules {
    kind: BuildingKind::WatchTower,
    cost: cost(0, 35, 0, 125),
    build_seconds: 80,
    max_hp: 850,
    side: 1,
    depth: 1,
    los: 10,
    population_support: 0,
    accepts: NONE,
    trainable: NO_UNITS,
    requires: None,
    buildable: true,
    blocks: true,
    food: 0,
    age: Age::Feudal,
};

pub(super) const GUARD_TOWER: BuildingRules = BuildingRules {
    kind: BuildingKind::GuardTower,
    max_hp: 1500,
    buildable: false,
    ..WATCH_TOWER
};

pub(super) const KEEP: BuildingRules = BuildingRules {
    kind: BuildingKind::Keep,
    max_hp: 2250,
    buildable: false,
    ..WATCH_TOWER
};
