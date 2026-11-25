//! Planning tables of `ai/aoe_ai/roster.py`.
use aoe_sdk::{Age, UnitKind};

use crate::roster::{land_soldier, profile, warship};

#[test]
fn land_soldiers_exclude_workers_scouts_generals_and_ships() {
    for kind in [
        UnitKind::Spearman,
        UnitKind::Archer,
        UnitKind::Skirmisher,
        UnitKind::Knight,
    ] {
        assert!(land_soldier(kind), "{kind:?}");
    }
    for kind in [
        UnitKind::Villager,
        UnitKind::Scout,
        UnitKind::General,
        UnitKind::WarGalley,
    ] {
        assert!(!land_soldier(kind), "{kind:?}");
    }
}

#[test]
fn warships_are_the_armed_naval_units() {
    assert!(warship(UnitKind::WarGalley));
    for kind in [
        UnitKind::Villager,
        UnitKind::Spearman,
        UnitKind::FishingShip,
    ] {
        assert!(!warship(kind), "{kind:?}");
    }
}

#[test]
fn a_profile_names_the_age_a_unit_needs() {
    assert_eq!(profile(UnitKind::Knight).age, Age::Castle);
    assert_eq!(profile(UnitKind::Spearman).age, Age::Feudal);
}
