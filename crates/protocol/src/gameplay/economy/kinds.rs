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
    TwoHandedSwordsman,
    Champion,
    Halberdier,
    Arbalester,
    Cavalier,
    Paladin,
    Hussar,
    CappedRam,
    SiegeRam,
    Onager,
    Steward,
    TradeCart,
    TradeCog,
}
impl UnitKind {
    pub const ALL: [Self; 37] = [
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
        Self::TwoHandedSwordsman,
        Self::Champion,
        Self::Halberdier,
        Self::Arbalester,
        Self::Cavalier,
        Self::Paladin,
        Self::Hussar,
        Self::CappedRam,
        Self::SiegeRam,
        Self::Onager,
        Self::Steward,
        Self::TradeCart,
        Self::TradeCog,
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
    StoneWall,
    StoneGate,
    Road,
    Wonder,
}
impl BuildingKind {
    pub const ALL: [Self; 25] = [
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
        Self::StoneWall,
        Self::StoneGate,
        Self::Road,
        Self::Wonder,
    ];
    /// Footprint side in tiles; the single wire-level size table.
    pub const fn side(self) -> i32 {
        match self {
            Self::Wonder => 5,
            Self::TownCenter
            | Self::Market
            | Self::PalisadeGate
            | Self::StoneGate
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
            Self::PalisadeWall
            | Self::StoneWall
            | Self::Outpost
            | Self::WatchTower
            | Self::Bridge
            | Self::Road => 1,
        }
    }
    /// A one-tile wall segment, laid by dragging a line.
    pub const fn is_wall(self) -> bool {
        matches!(self, Self::PalisadeWall | Self::StoneWall)
    }
    pub const fn is_line(self) -> bool {
        self.is_wall() || matches!(self, Self::Road)
    }
    pub const fn is_gate(self) -> bool {
        matches!(self, Self::PalisadeGate | Self::StoneGate)
    }
    pub const fn depth(self) -> i32 {
        if matches!(self, Self::PalisadeGate | Self::StoneGate) {
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
    Imperial,
}
/// Bit set of researched technologies: bit `tech as u8`.
pub type TechMask = u128;
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
    ImperialAge,
    TwoHandedSwordsman,
    Champion,
    Halberdier,
    Arbalester,
    Cavalier,
    Paladin,
    Hussar,
    CappedRam,
    SiegeRam,
    Onager,
    BlastFurnace,
    PlateMailArmor,
    PlateBardingArmor,
    Bracer,
    RingArcherArmor,
    CropRotation,
    TwoManSaw,
    StoneShaftMining,
    Squires,
    Arson,
    Gambesons,
    ThumbRing,
    ParthianTactics,
    Bloodlines,
    Husbandry,
    Architecture,
    SiegeEngineers,
    HeatedShot,
    Arrowslits,
    FortifiedWall,
    GuardTower,
    Keep,
    Devotion,
    Faith,
    Fervor,
    Sanctity,
    HerbalMedicine,
    Theocracy,
    Illumination,
    BlockPrinting,
    Atonement,
    Redemption,
    Heresy,
    Conscription,
    Hoardings,
    Sappers,
    Spies,
    Caravan,
    Guilds,
    Banking,
    Coinage,
    TreadmillCrane,
}
impl Tech {
    pub const ALL: [Tech; 87] = [
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
        Tech::ImperialAge,
        Tech::TwoHandedSwordsman,
        Tech::Champion,
        Tech::Halberdier,
        Tech::Arbalester,
        Tech::Cavalier,
        Tech::Paladin,
        Tech::Hussar,
        Tech::CappedRam,
        Tech::SiegeRam,
        Tech::Onager,
        Tech::BlastFurnace,
        Tech::PlateMailArmor,
        Tech::PlateBardingArmor,
        Tech::Bracer,
        Tech::RingArcherArmor,
        Tech::CropRotation,
        Tech::TwoManSaw,
        Tech::StoneShaftMining,
        Tech::Squires,
        Tech::Arson,
        Tech::Gambesons,
        Tech::ThumbRing,
        Tech::ParthianTactics,
        Tech::Bloodlines,
        Tech::Husbandry,
        Tech::Architecture,
        Tech::SiegeEngineers,
        Tech::HeatedShot,
        Tech::Arrowslits,
        Tech::FortifiedWall,
        Tech::GuardTower,
        Tech::Keep,
        Tech::Devotion,
        Tech::Faith,
        Tech::Fervor,
        Tech::Sanctity,
        Tech::HerbalMedicine,
        Tech::Theocracy,
        Tech::Illumination,
        Tech::BlockPrinting,
        Tech::Atonement,
        Tech::Redemption,
        Tech::Heresy,
        Tech::Conscription,
        Tech::Hoardings,
        Tech::Sappers,
        Tech::Spies,
        Tech::Caravan,
        Tech::Guilds,
        Tech::Banking,
        Tech::Coinage,
        Tech::TreadmillCrane,
    ];
    pub const fn building(self) -> BuildingKind {
        match self {
            Self::WarGalley => BuildingKind::Dock,
            Self::ImperialAge => BuildingKind::TownCenter,
            Self::TwoHandedSwordsman | Self::Champion | Self::Halberdier => BuildingKind::Barracks,
            Self::Arbalester => BuildingKind::ArcheryRange,
            Self::Cavalier | Self::Paladin | Self::Hussar => BuildingKind::Stable,
            Self::CappedRam | Self::SiegeRam | Self::Onager => BuildingKind::SiegeWorkshop,
            Self::BlastFurnace
            | Self::PlateMailArmor
            | Self::PlateBardingArmor
            | Self::Bracer
            | Self::RingArcherArmor => BuildingKind::Blacksmith,
            Self::CropRotation => BuildingKind::Mill,
            Self::TwoManSaw => BuildingKind::LumberCamp,
            Self::StoneShaftMining => BuildingKind::MiningCamp,
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
            Self::Squires | Self::Arson | Self::Gambesons => BuildingKind::Barracks,
            Self::ThumbRing | Self::ParthianTactics => BuildingKind::ArcheryRange,
            Self::Bloodlines | Self::Husbandry => BuildingKind::Stable,
            Self::Architecture
            | Self::SiegeEngineers
            | Self::HeatedShot
            | Self::Arrowslits
            | Self::FortifiedWall
            | Self::GuardTower
            | Self::Keep
            | Self::TreadmillCrane => BuildingKind::University,
            Self::Devotion
            | Self::Faith
            | Self::Fervor
            | Self::Sanctity
            | Self::HerbalMedicine
            | Self::Theocracy
            | Self::Illumination
            | Self::BlockPrinting
            | Self::Atonement
            | Self::Redemption
            | Self::Heresy => BuildingKind::Monastery,
            Self::Conscription | Self::Hoardings | Self::Sappers | Self::Spies => {
                BuildingKind::Castle
            }
            Self::Caravan | Self::Guilds | Self::Banking | Self::Coinage => BuildingKind::Market,
            Self::Forging
            | Self::ScaleMailArmor
            | Self::ScaleBardingArmor
            | Self::Fletching
            | Self::PaddedArcherArmor => BuildingKind::Blacksmith,
        }
    }
    /// The Age a technology needs, for wire validation. Original techs keep
    /// their index bands; later ones are listed.
    pub const fn min_age(self) -> Age {
        match self {
            Self::Arson | Self::Bloodlines => Age::Feudal,
            Self::Squires
            | Self::Gambesons
            | Self::ThumbRing
            | Self::Husbandry
            | Self::HeatedShot
            | Self::FortifiedWall
            | Self::GuardTower
            | Self::Devotion
            | Self::Fervor
            | Self::Sanctity
            | Self::HerbalMedicine
            | Self::Atonement
            | Self::Redemption
            | Self::Heresy
            | Self::Caravan
            | Self::Coinage
            | Self::TreadmillCrane => Age::Castle,
            Self::ParthianTactics
            | Self::Architecture
            | Self::SiegeEngineers
            | Self::Arrowslits
            | Self::Keep
            | Self::Faith
            | Self::Theocracy
            | Self::Illumination
            | Self::BlockPrinting
            | Self::Conscription
            | Self::Hoardings
            | Self::Sappers
            | Self::Spies
            | Self::Guilds
            | Self::Banking => Age::Imperial,
            Self::Loom => Age::Dark,
            _ if (self as u8) >= Self::ImperialAge as u8 && (self as u8) < 53 => Age::Imperial,
            _ if (self as u8) >= Self::LongSwordsman as u8 && (self as u8) < 53 => Age::Castle,
            _ => Age::Feudal,
        }
    }
    pub const fn bit(self) -> TechMask {
        1 << self as u8
    }
}
