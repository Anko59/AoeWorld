use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum UnitKind {
    Villager,
    Scout,
    Militia,
    FishingShip,
    Archer,
    Skirmisher,
    Spearman,
    ManAtArms,
    LongSwordsman,
    Pikeman,
    Crossbowman,
    EliteSkirmisher,
    Knight,
    LightCavalry,
    Monk,
    Mangonel,
    BatteringRam,
    Galley,
    FireShip,
    DemolitionShip,
    TransportShip,
    WarGalley,
    Vassal,
    General,
}
impl UnitKind {
    pub const ALL: [Self; 24] = [
        Self::Villager,
        Self::Scout,
        Self::Militia,
        Self::FishingShip,
        Self::Archer,
        Self::Skirmisher,
        Self::Spearman,
        Self::ManAtArms,
        Self::LongSwordsman,
        Self::Pikeman,
        Self::Crossbowman,
        Self::EliteSkirmisher,
        Self::Knight,
        Self::LightCavalry,
        Self::Monk,
        Self::Mangonel,
        Self::BatteringRam,
        Self::Galley,
        Self::FireShip,
        Self::DemolitionShip,
        Self::TransportShip,
        Self::WarGalley,
        Self::Vassal,
        Self::General,
    ];
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum BuildingKind {
    TownCenter,
    House,
    LumberCamp,
    MiningCamp,
    Mill,
    Farm,
    Barracks,
    PalisadeWall,
    Outpost,
    Dock,
    ArcheryRange,
    Stable,
    Blacksmith,
    Market,
    WatchTower,
    PalisadeGate,
    Castle,
    Monastery,
    University,
    SiegeWorkshop,
    Bridge,
}
impl BuildingKind {
    pub const ALL: [Self; 21] = [
        Self::TownCenter,
        Self::House,
        Self::LumberCamp,
        Self::MiningCamp,
        Self::Mill,
        Self::Farm,
        Self::Barracks,
        Self::PalisadeWall,
        Self::Outpost,
        Self::Dock,
        Self::ArcheryRange,
        Self::Stable,
        Self::Blacksmith,
        Self::Market,
        Self::WatchTower,
        Self::PalisadeGate,
        Self::Castle,
        Self::Monastery,
        Self::University,
        Self::SiegeWorkshop,
        Self::Bridge,
    ];
    /// Footprint side in tiles; the single wire-level size table.
    pub const fn side(self) -> i32 {
        match self {
            Self::TownCenter
            | Self::Market
            | Self::PalisadeGate
            | Self::Castle
            | Self::University
            | Self::SiegeWorkshop => 4,
            Self::House | Self::LumberCamp | Self::MiningCamp | Self::Mill => 2,
            Self::Farm
            | Self::Barracks
            | Self::Dock
            | Self::ArcheryRange
            | Self::Stable
            | Self::Blacksmith
            | Self::Monastery => 3,
            Self::PalisadeWall | Self::Outpost | Self::WatchTower | Self::Bridge => 1,
        }
    }
    pub const fn depth(self) -> i32 {
        if matches!(self, Self::PalisadeGate) {
            1
        } else {
            self.side()
        }
    }
}
/// The player's age; Feudal gates buildings and economy research.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Age {
    Dark,
    Feudal,
    Castle,
}
/// A technology; bit `1 << tech as u8` of `GameplayPlayerState::researched`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Tech {
    Loom,
    TownWatch,
    FeudalAge,
    DoubleBitAxe,
    HorseCollar,
    GoldMining,
    StoneMining,
    Wheelbarrow,
    ManAtArms,
    Forging,
    ScaleMailArmor,
    ScaleBardingArmor,
    Fletching,
    PaddedArcherArmor,
    CastleAge,
    LongSwordsman,
    Pikeman,
    Crossbowman,
    EliteSkirmisher,
    LightCavalry,
    HandCart,
    TownPatrol,
    IronCasting,
    ChainMailArmor,
    ChainBardingArmor,
    BodkinArrow,
    LeatherArcherArmor,
    HeavyPlow,
    BowSaw,
    GoldShaftMining,
    Masonry,
    Ballistics,
    MurderHoles,
    WarGalley,
}
impl Tech {
    pub const ALL: [Tech; 34] = [
        Tech::Loom,
        Tech::TownWatch,
        Tech::FeudalAge,
        Tech::DoubleBitAxe,
        Tech::HorseCollar,
        Tech::GoldMining,
        Tech::StoneMining,
        Tech::Wheelbarrow,
        Tech::ManAtArms,
        Tech::Forging,
        Tech::ScaleMailArmor,
        Tech::ScaleBardingArmor,
        Tech::Fletching,
        Tech::PaddedArcherArmor,
        Tech::CastleAge,
        Tech::LongSwordsman,
        Tech::Pikeman,
        Tech::Crossbowman,
        Tech::EliteSkirmisher,
        Tech::LightCavalry,
        Tech::HandCart,
        Tech::TownPatrol,
        Tech::IronCasting,
        Tech::ChainMailArmor,
        Tech::ChainBardingArmor,
        Tech::BodkinArrow,
        Tech::LeatherArcherArmor,
        Tech::HeavyPlow,
        Tech::BowSaw,
        Tech::GoldShaftMining,
        Tech::Masonry,
        Tech::Ballistics,
        Tech::MurderHoles,
        Tech::WarGalley,
    ];
    pub const fn building(self) -> BuildingKind {
        match self {
            Self::WarGalley => BuildingKind::Dock,
            Self::CastleAge | Self::HandCart | Self::TownPatrol => BuildingKind::TownCenter,
            Self::LongSwordsman | Self::Pikeman => BuildingKind::Barracks,
            Self::Crossbowman | Self::EliteSkirmisher => BuildingKind::ArcheryRange,
            Self::LightCavalry => BuildingKind::Stable,
            Self::IronCasting
            | Self::ChainMailArmor
            | Self::ChainBardingArmor
            | Self::BodkinArrow
            | Self::LeatherArcherArmor => BuildingKind::Blacksmith,
            Self::HeavyPlow => BuildingKind::Mill,
            Self::BowSaw => BuildingKind::LumberCamp,
            Self::GoldShaftMining => BuildingKind::MiningCamp,
            Self::Masonry | Self::Ballistics | Self::MurderHoles => BuildingKind::University,
            Self::Loom | Self::TownWatch | Self::FeudalAge | Self::Wheelbarrow => {
                BuildingKind::TownCenter
            }
            Self::DoubleBitAxe => BuildingKind::LumberCamp,
            Self::HorseCollar => BuildingKind::Mill,
            Self::GoldMining | Self::StoneMining => BuildingKind::MiningCamp,
            Self::ManAtArms => BuildingKind::Barracks,
            Self::Forging
            | Self::ScaleMailArmor
            | Self::ScaleBardingArmor
            | Self::Fletching
            | Self::PaddedArcherArmor => BuildingKind::Blacksmith,
        }
    }
    pub const fn bit(self) -> u64 {
        1 << self as u8
    }
}
