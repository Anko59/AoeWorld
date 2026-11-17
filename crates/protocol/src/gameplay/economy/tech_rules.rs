use super::kinds::{Age, BuildingKind, Tech};

impl Tech {
    pub const fn building(self) -> BuildingKind {
        match self {
            Self::WarGalley
            | Self::FishingLines
            | Self::Gillnets
            | Self::HeavyWarships
            | Self::DemolitionShip
            | Self::HeavyDemolitionShip
            | Self::EliteCannonGalleon => BuildingKind::Dock,
            Self::Careening
            | Self::DryDock
            | Self::Shipwright
            | Self::ClinkerConstruction
            | Self::CarvelHull
            | Self::Siphons
            | Self::Incendiaries => BuildingKind::University,
            Self::Chemistry => BuildingKind::University,
            Self::HeavyCavalryArcher => BuildingKind::ArcheryRange,
            Self::HeavyCamelRider => BuildingKind::Stable,
            Self::HeavyScorpion | Self::SiegeOnager => BuildingKind::SiegeWorkshop,
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
            Self::Arson | Self::Bloodlines | Self::FishingLines => Age::Feudal,
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
            | Self::TreadmillCrane
            | Self::Gillnets
            | Self::Careening
            | Self::ClinkerConstruction
            | Self::Siphons
            | Self::DemolitionShip => Age::Castle,
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
            | Self::Banking
            | Self::DryDock
            | Self::Shipwright
            | Self::CarvelHull
            | Self::Incendiaries
            | Self::HeavyWarships
            | Self::HeavyDemolitionShip
            | Self::EliteCannonGalleon
            | Self::Chemistry
            | Self::HeavyCavalryArcher
            | Self::HeavyCamelRider
            | Self::HeavyScorpion
            | Self::SiegeOnager => Age::Imperial,
            Self::Loom => Age::Dark,
            _ if (self as u8) >= Self::ImperialAge as u8 && (self as u8) < 53 => Age::Imperial,
            _ if (self as u8) >= Self::LongSwordsman as u8 && (self as u8) < 53 => Age::Castle,
            _ => Age::Feudal,
        }
    }
}
