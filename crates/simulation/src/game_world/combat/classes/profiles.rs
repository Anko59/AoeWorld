//! Non-base classes that have attackers or victims in the current roster.
use crate::{BuildingKind, UnitKind};

#[derive(Clone, Copy)]
#[repr(u8)]
pub(super) enum Class {
    Infantry = 1,
    Cavalry = 8,
    Building = 11,
    StoneDefense = 13,
    Archer = 15,
    Ship = 16,
    Ram = 17,
    Siege = 20,
    StandardBuilding = 21,
    Wall = 22,
    Gunpowder = 23,
    Monk = 25,
    Castle = 26,
    Spear = 27,
    CavalryArcher = 28,
    Camel = 30,
    FishingShip = 34,
    Skirmisher = 38,
    /// The Fire Ship line's own armour class, which the Hulk line counters.
    FireShipLine = 41,
    /// Galleys, galleons and cannon ships: the single warship bonus class.
    LongRangedShip = 60,
}
use Class::*;
type Classes = &'static [(Class, i16)];

pub(super) const fn unit_armour(kind: UnitKind) -> Classes {
    match kind {
        UnitKind::Villager | UnitKind::Vassal | UnitKind::Steward | UnitKind::TradeCart => &[],
        // The mounted General takes cavalry bonuses, so spears and pikes counter it.
        UnitKind::Scout
        | UnitKind::Knight
        | UnitKind::LightCavalry
        | UnitKind::Cavalier
        | UnitKind::Paladin
        | UnitKind::Hussar
        | UnitKind::General => &[(Cavalry, 0)],
        UnitKind::Militia
        | UnitKind::ManAtArms
        | UnitKind::LongSwordsman
        | UnitKind::TwoHandedSwordsman
        | UnitKind::Champion => &[(Infantry, 0)],
        UnitKind::FishingShip => &[(FishingShip, 0)],
        UnitKind::Archer | UnitKind::Crossbowman | UnitKind::Arbalester => &[(Archer, 0)],
        UnitKind::Skirmisher | UnitKind::EliteSkirmisher => &[(Archer, 0), (Skirmisher, 0)],
        UnitKind::Spearman | UnitKind::Pikeman | UnitKind::Halberdier => {
            &[(Infantry, 0), (Spear, 0)]
        }
        UnitKind::Monk => &[(Monk, 0)],
        UnitKind::Mangonel | UnitKind::Onager => &[(Siege, 0)],
        UnitKind::BatteringRam | UnitKind::CappedRam | UnitKind::SiegeRam => {
            &[(Siege, 0), (Ram, 0)]
        }
        // Naval Warfare Overhaul (update 169123): one warship bonus class.
        UnitKind::Galley
        | UnitKind::WarGalley
        | UnitKind::Galleon
        | UnitKind::CannonGalleon
        | UnitKind::EliteCannonGalleon
        | UnitKind::CatapultGalleon => &[(Ship, 0), (LongRangedShip, 0)],
        UnitKind::FireGalley | UnitKind::FireShip | UnitKind::FastFireShip => {
            &[(Ship, 0), (FireShipLine, 0)]
        }
        UnitKind::TransportShip
        | UnitKind::TradeCog
        | UnitKind::DemolitionRaft
        | UnitKind::DemolitionShip
        | UnitKind::HeavyDemolitionShip
        | UnitKind::Hulk
        | UnitKind::WarHulk
        | UnitKind::Carrack => &[(Ship, 0)],
        UnitKind::CavalryArcher | UnitKind::HeavyCavalryArcher => {
            &[(Archer, 0), (Cavalry, 0), (CavalryArcher, 0)]
        }
        UnitKind::CamelRider | UnitKind::HeavyCamelRider => &[(Camel, 0)],
        UnitKind::Scorpion
        | UnitKind::HeavyScorpion
        | UnitKind::Trebuchet
        | UnitKind::SiegeOnager => &[(Siege, 0)],
        UnitKind::TrebuchetUnpacked => &[(Siege, 0), (Ram, 0)],
        UnitKind::HandCannoneer => &[(Archer, 0), (Gunpowder, 0)],
        UnitKind::BombardCannon => &[(Siege, 0), (Gunpowder, 0)],
        UnitKind::Petard => &[],
    }
}

pub(super) const fn unit_attacks(kind: UnitKind) -> Classes {
    match kind {
        UnitKind::Villager => &[(Building, 3), (StoneDefense, 6)],
        UnitKind::Scout => &[(Monk, 6)],
        UnitKind::Spearman => &[
            (Cavalry, 15),
            (Camel, 12),
            (Ship, 9),
            (FishingShip, 9),
            (StandardBuilding, 1),
        ],
        UnitKind::Pikeman => &[
            (Cavalry, 22),
            (Camel, 18),
            (Ship, 16),
            (FishingShip, 16),
            (StandardBuilding, 1),
        ],
        UnitKind::Skirmisher => &[(Archer, 3), (Spear, 3)],
        UnitKind::EliteSkirmisher => &[(Archer, 4), (Spear, 4), (CavalryArcher, 2)],
        UnitKind::Archer | UnitKind::Crossbowman | UnitKind::Arbalester => &[(Spear, 3)],
        UnitKind::ManAtArms => &[(StandardBuilding, 2)],
        UnitKind::LongSwordsman => &[(StandardBuilding, 3)],
        UnitKind::TwoHandedSwordsman | UnitKind::Champion => &[(StandardBuilding, 4)],
        UnitKind::LightCavalry => &[(Monk, 10)],
        UnitKind::Hussar => &[(Monk, 12)],
        UnitKind::Halberdier => &[
            (Cavalry, 32),
            (Camel, 26),
            (Ship, 17),
            (FishingShip, 26),
            (StandardBuilding, 1),
        ],
        UnitKind::BatteringRam => &[(Building, 150), (Siege, 40)],
        UnitKind::CappedRam => &[(Building, 160), (Siege, 50)],
        UnitKind::SiegeRam => &[(Building, 200), (Siege, 65)],
        UnitKind::Mangonel => &[(Building, 35), (Siege, 12)],
        UnitKind::Onager => &[(Building, 45), (Siege, 12)],
        UnitKind::Galley | UnitKind::WarGalley => &[(Building, 6), (Ram, 3), (LongRangedShip, 5)],
        UnitKind::Galleon => &[(Building, 6), (Ram, 3), (LongRangedShip, 9)],
        UnitKind::FireGalley => &[(LongRangedShip, 1)],
        UnitKind::FireShip => &[(Building, 2), (LongRangedShip, 1)],
        UnitKind::FastFireShip => &[(Building, 3), (LongRangedShip, 1)],
        UnitKind::DemolitionRaft => &[(Building, 180)],
        UnitKind::DemolitionShip => &[(Building, 220)],
        UnitKind::HeavyDemolitionShip => &[(Building, 280)],
        UnitKind::Hulk | UnitKind::WarHulk | UnitKind::Carrack => &[(FireShipLine, 1)],
        UnitKind::CannonGalleon => &[(Building, 200), (Siege, 25)],
        UnitKind::EliteCannonGalleon => &[(Building, 275), (Siege, 25)],
        UnitKind::CatapultGalleon => &[(Building, 130), (Siege, 10)],
        UnitKind::CavalryArcher => &[(Spear, 2)],
        UnitKind::HeavyCavalryArcher => &[(Spear, 4)],
        UnitKind::CamelRider => &[(Cavalry, 9), (Ship, 5), (Camel, 5)],
        UnitKind::HeavyCamelRider => &[(Cavalry, 18), (Ship, 9), (Camel, 9)],
        UnitKind::Scorpion => &[(Building, 3), (Infantry, 1), (Ram, 1)],
        UnitKind::HeavyScorpion => &[(Building, 6), (Infantry, 2), (Ram, 2)],
        UnitKind::HandCannoneer => &[(Infantry, 10), (Spear, 1), (Ram, 2)],
        UnitKind::BombardCannon => &[(Building, 200), (Ship, 40), (Siege, 20), (StoneDefense, 40)],
        UnitKind::Trebuchet | UnitKind::TrebuchetUnpacked => &[(Building, 250)],
        UnitKind::Petard => &[(Castle, 100), (Building, 500), (Siege, 60), (Wall, 900)],
        UnitKind::SiegeOnager => &[(Building, 60), (Siege, 12)],
        UnitKind::Militia
        | UnitKind::FishingShip
        | UnitKind::Knight
        | UnitKind::Cavalier
        | UnitKind::Paladin
        | UnitKind::Monk
        | UnitKind::TransportShip
        | UnitKind::Vassal
        | UnitKind::Steward
        | UnitKind::TradeCart
        | UnitKind::TradeCog
        | UnitKind::General => &[],
    }
}

pub(super) const fn building_armour(kind: BuildingKind) -> Classes {
    match kind {
        BuildingKind::Castle => &[(Building, 8), (StandardBuilding, 0), (Castle, 0)],
        BuildingKind::Wonder => &[(Building, 0)],
        // DAT 199: Building armour class 10.
        BuildingKind::FishTrap => &[(Building, 10), (StandardBuilding, 0)],
        BuildingKind::WatchTower => &[(Building, 0), (StandardBuilding, 0), (StoneDefense, 0)],
        BuildingKind::PalisadeWall
        | BuildingKind::PalisadeGate
        | BuildingKind::StoneWall
        | BuildingKind::StoneGate
        | BuildingKind::Bridge => &[(Building, 0), (StandardBuilding, 0), (Wall, 0)],
        BuildingKind::Road
        | BuildingKind::TownCenter
        | BuildingKind::House
        | BuildingKind::LumberCamp
        | BuildingKind::MiningCamp
        | BuildingKind::Mill
        | BuildingKind::Farm
        | BuildingKind::Barracks
        | BuildingKind::Outpost
        | BuildingKind::Dock
        | BuildingKind::ArcheryRange
        | BuildingKind::Stable
        | BuildingKind::Blacksmith
        | BuildingKind::Market
        | BuildingKind::Monastery
        | BuildingKind::University
        | BuildingKind::SiegeWorkshop => &[(Building, 0), (StandardBuilding, 0)],
    }
}

pub(super) const fn building_attacks(kind: BuildingKind, secondary: bool) -> Classes {
    if secondary {
        match kind {
            BuildingKind::WatchTower => return &[(StoneDefense, 5), (Ship, 7), (FishingShip, 7)],
            BuildingKind::Castle => return &[(StoneDefense, 11), (Ship, 11), (FishingShip, 11)],
            _ => {}
        }
    }
    match kind {
        // Town Centers fire only secondary arrows, which carry these attacks.
        BuildingKind::Road => &[(Building, 0), (StandardBuilding, 0)],
        BuildingKind::TownCenter => &[(Building, 5), (Ship, 5), (FishingShip, 5)],
        BuildingKind::WatchTower => &[(Spear, 2), (Ship, 7), (FishingShip, 7)],
        BuildingKind::Castle => &[(Spear, 2)],
        _ => &[],
    }
}
