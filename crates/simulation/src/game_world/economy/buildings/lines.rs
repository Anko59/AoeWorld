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
    UnitKind::CavalryArcher,
    UnitKind::HeavyCavalryArcher,
    UnitKind::HandCannoneer,
];
pub(super) const STABLE: &[UnitKind] = &[
    UnitKind::Scout,
    UnitKind::Knight,
    UnitKind::LightCavalry,
    UnitKind::Cavalier,
    UnitKind::Paladin,
    UnitKind::Hussar,
    UnitKind::CamelRider,
    UnitKind::HeavyCamelRider,
];
pub(super) const SIEGE: &[UnitKind] = &[
    UnitKind::Mangonel,
    UnitKind::BatteringRam,
    UnitKind::CappedRam,
    UnitKind::SiegeRam,
    UnitKind::Onager,
    UnitKind::Scorpion,
    UnitKind::HeavyScorpion,
    UnitKind::BombardCannon,
    UnitKind::SiegeOnager,
];
/// Every ship the Dock trains; upgraded tiers are hidden once superseded.
pub(super) const DOCK: &[UnitKind] = &[
    UnitKind::FishingShip,
    UnitKind::TradeCog,
    UnitKind::Galley,
    UnitKind::FireShip,
    UnitKind::DemolitionShip,
    UnitKind::TransportShip,
    UnitKind::WarGalley,
    UnitKind::FireGalley,
    UnitKind::FastFireShip,
    UnitKind::DemolitionRaft,
    UnitKind::HeavyDemolitionShip,
    UnitKind::Galleon,
    UnitKind::CannonGalleon,
    UnitKind::EliteCannonGalleon,
    UnitKind::Hulk,
    UnitKind::WarHulk,
    UnitKind::Carrack,
    UnitKind::CatapultGalleon,
];
