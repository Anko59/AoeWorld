//! Unit identities each production building trains, base kind first and the
//! newest upgrade of every line last.
use super::UnitKind;

pub(super) const MILITIA: &[UnitKind] = &[
    UnitKind::Militia,
    UnitKind::Spearman,
    UnitKind::ManAtArms,
    UnitKind::LongSwordsman,
    UnitKind::Pikeman,
    UnitKind::TwoHandedSwordsman,
    UnitKind::Champion,
    UnitKind::Halberdier,
];
pub(super) const RANGE: &[UnitKind] = &[
    UnitKind::Archer,
    UnitKind::Skirmisher,
    UnitKind::Crossbowman,
    UnitKind::EliteSkirmisher,
    UnitKind::Arbalester,
];
pub(super) const STABLE: &[UnitKind] = &[
    UnitKind::Scout,
    UnitKind::Knight,
    UnitKind::LightCavalry,
    UnitKind::Cavalier,
    UnitKind::Paladin,
    UnitKind::Hussar,
];
pub(super) const SIEGE: &[UnitKind] = &[
    UnitKind::Mangonel,
    UnitKind::BatteringRam,
    UnitKind::CappedRam,
    UnitKind::SiegeRam,
    UnitKind::Onager,
];
