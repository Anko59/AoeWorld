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
    Monk = 25,
    Castle = 26,
    Spear = 27,
    FishingShip = 34,
    Skirmisher = 38,
}
use Class::*;
type Classes = &'static [(Class, i16)];

pub(super) const fn unit_armour(kind: UnitKind) -> Classes {
    match kind {
        UnitKind::Villager | UnitKind::Vassal => &[],
        // The mounted General takes cavalry bonuses, so spears and pikes counter it.
        UnitKind::Scout | UnitKind::Knight | UnitKind::LightCavalry | UnitKind::General => {
            &[(Cavalry, 0)]
        }
        UnitKind::Militia | UnitKind::ManAtArms | UnitKind::LongSwordsman => &[(Infantry, 0)],
        UnitKind::FishingShip => &[(FishingShip, 0)],
        UnitKind::Archer | UnitKind::Crossbowman => &[(Archer, 0)],
        UnitKind::Skirmisher | UnitKind::EliteSkirmisher => &[(Archer, 0), (Skirmisher, 0)],
        UnitKind::Spearman | UnitKind::Pikeman => &[(Infantry, 0), (Spear, 0)],
        UnitKind::Monk => &[(Monk, 0)],
        UnitKind::Mangonel => &[(Siege, 0)],
        UnitKind::BatteringRam => &[(Siege, 0), (Ram, 0)],
        UnitKind::Galley | UnitKind::WarGalley | UnitKind::TransportShip => &[(Ship, 0)],
        UnitKind::FireShip => &[(Ship, 6)],
        UnitKind::DemolitionShip => &[(Ship, 3)],
    }
}

pub(super) const fn unit_attacks(kind: UnitKind) -> Classes {
    match kind {
        UnitKind::Villager => &[(Building, 3), (StoneDefense, 6)],
        UnitKind::Scout => &[(Monk, 6)],
        UnitKind::Spearman => &[
            (Cavalry, 15),
            (Ship, 9),
            (FishingShip, 9),
            (StandardBuilding, 1),
        ],
        UnitKind::Pikeman => &[
            (Cavalry, 22),
            (Ship, 16),
            (FishingShip, 16),
            (StandardBuilding, 1),
        ],
        UnitKind::Skirmisher => &[(Archer, 3), (Spear, 3)],
        UnitKind::EliteSkirmisher => &[(Archer, 4), (Spear, 4)],
        UnitKind::Archer | UnitKind::Crossbowman => &[(Spear, 3)],
        UnitKind::ManAtArms => &[(StandardBuilding, 2)],
        UnitKind::LongSwordsman => &[(StandardBuilding, 3)],
        UnitKind::LightCavalry => &[(Monk, 10)],
        UnitKind::BatteringRam => &[(Building, 150), (Siege, 40)],
        UnitKind::Mangonel => &[(Building, 35), (Siege, 12)],
        UnitKind::Galley => &[(Building, 6), (Ship, 8), (FishingShip, 8), (Ram, 3)],
        UnitKind::WarGalley => &[(Building, 7), (Ship, 9), (FishingShip, 9), (Ram, 4)],
        UnitKind::FireShip => &[(Building, 2), (Ship, 3), (FishingShip, 3)],
        UnitKind::DemolitionShip => &[(Building, 220)],
        UnitKind::Militia
        | UnitKind::FishingShip
        | UnitKind::Knight
        | UnitKind::Monk
        | UnitKind::TransportShip
        | UnitKind::Vassal
        | UnitKind::General => &[],
    }
}

pub(super) const fn building_armour(kind: BuildingKind) -> Classes {
    match kind {
        BuildingKind::Castle => &[(Building, 8), (StandardBuilding, 0), (Castle, 0)],
        BuildingKind::WatchTower => &[(Building, 0), (StandardBuilding, 0), (StoneDefense, 0)],
        BuildingKind::PalisadeWall | BuildingKind::PalisadeGate | BuildingKind::Bridge => {
            &[(Building, 0), (StandardBuilding, 0), (Wall, 0)]
        }
        BuildingKind::TownCenter
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
        BuildingKind::TownCenter => &[(Building, 5), (Ship, 5), (FishingShip, 5)],
        BuildingKind::WatchTower => &[(Spear, 2), (Ship, 7), (FishingShip, 7)],
        BuildingKind::Castle => &[(Spear, 2)],
        _ => &[],
    }
}
