//! The game's public rules tables (costs and times), so an AI can plan without
//! trial and error. They are the same tables the server charges from.
use crate::model::{BuildingKind, Stockpile, Tech, UnitKind};
use aoe_simulation as sim;

mod production;
pub use production::{
    UnitProfile, training_options, unit_building_bonus, unit_counter_bonus, unit_profile,
};

fn stock(cost: sim::Stockpile) -> Stockpile {
    Stockpile {
        food: cost.food,
        wood: cost.wood,
        gold: cost.gold,
        stone: cost.stone,
    }
}

pub(crate) fn native_building(kind: BuildingKind) -> sim::BuildingKind {
    use sim::BuildingKind as N;
    match kind {
        BuildingKind::TownCenter => N::TownCenter,
        BuildingKind::House => N::House,
        BuildingKind::LumberCamp => N::LumberCamp,
        BuildingKind::MiningCamp => N::MiningCamp,
        BuildingKind::Mill => N::Mill,
        BuildingKind::Farm => N::Farm,
        BuildingKind::Barracks => N::Barracks,
        BuildingKind::PalisadeWall => N::PalisadeWall,
        BuildingKind::Outpost => N::Outpost,
        BuildingKind::Dock => N::Dock,
        BuildingKind::ArcheryRange => N::ArcheryRange,
        BuildingKind::Stable => N::Stable,
        BuildingKind::Blacksmith => N::Blacksmith,
        BuildingKind::Market => N::Market,
        BuildingKind::WatchTower => N::WatchTower,
        BuildingKind::GuardTower => N::GuardTower,
        BuildingKind::Keep => N::Keep,
        BuildingKind::PalisadeGate => N::PalisadeGate,
        BuildingKind::Castle => N::Castle,
        BuildingKind::Monastery => N::Monastery,
        BuildingKind::University => N::University,
        BuildingKind::SiegeWorkshop => N::SiegeWorkshop,
        BuildingKind::Bridge => N::Bridge,
        BuildingKind::StoneWall => N::StoneWall,
        BuildingKind::StoneGate => N::StoneGate,
        BuildingKind::Road => N::Road,
        BuildingKind::Wonder => N::Wonder,
        BuildingKind::FishTrap => N::FishTrap,
    }
}

// Use one exhaustive mapping in both directions. The enum ordinals belong to
// their own representations and need not match.
macro_rules! unit_identities {
    ($($unit:ident),+ $(,)?) => {
        fn native_unit(kind: UnitKind) -> sim::UnitKind {
            match kind {
                $(UnitKind::$unit => sim::UnitKind::$unit,)+
            }
        }

        fn public_unit(kind: sim::UnitKind) -> UnitKind {
            match kind {
                $(sim::UnitKind::$unit => UnitKind::$unit,)+
            }
        }
    };
}

unit_identities!(
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
    Steward,
    TradeCart,
    TradeCog,
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
    FireGalley,
    FastFireShip,
    DemolitionRaft,
    HeavyDemolitionShip,
    Galleon,
    CannonGalleon,
    EliteCannonGalleon,
    Hulk,
    WarHulk,
    Carrack,
    CatapultGalleon,
    CavalryArcher,
    HeavyCavalryArcher,
    CamelRider,
    HeavyCamelRider,
    Scorpion,
    HeavyScorpion,
    HandCannoneer,
    BombardCannon,
    Trebuchet,
    TrebuchetUnpacked,
    Petard,
    SiegeOnager,
);

fn native_tech(tech: Tech) -> sim::Tech {
    match tech {
        Tech::Loom => sim::Tech::Loom,
        Tech::FishingLines => sim::Tech::FishingLines,
        Tech::Gillnets => sim::Tech::Gillnets,
        Tech::Careening => sim::Tech::Careening,
        Tech::DryDock => sim::Tech::DryDock,
        Tech::Shipwright => sim::Tech::Shipwright,
        Tech::ClinkerConstruction => sim::Tech::ClinkerConstruction,
        Tech::CarvelHull => sim::Tech::CarvelHull,
        Tech::Siphons => sim::Tech::Siphons,
        Tech::Incendiaries => sim::Tech::Incendiaries,
        Tech::HeavyWarships => sim::Tech::HeavyWarships,
        Tech::DemolitionShip => sim::Tech::DemolitionShip,
        Tech::HeavyDemolitionShip => sim::Tech::HeavyDemolitionShip,
        Tech::EliteCannonGalleon => sim::Tech::EliteCannonGalleon,
        Tech::TownWatch => sim::Tech::TownWatch,
        Tech::FeudalAge => sim::Tech::FeudalAge,
        Tech::DoubleBitAxe => sim::Tech::DoubleBitAxe,
        Tech::HorseCollar => sim::Tech::HorseCollar,
        Tech::GoldMining => sim::Tech::GoldMining,
        Tech::StoneMining => sim::Tech::StoneMining,
        Tech::Wheelbarrow => sim::Tech::Wheelbarrow,
        Tech::ManAtArms => sim::Tech::ManAtArms,
        Tech::Forging => sim::Tech::Forging,
        Tech::ScaleMailArmor => sim::Tech::ScaleMailArmor,
        Tech::ScaleBardingArmor => sim::Tech::ScaleBardingArmor,
        Tech::Fletching => sim::Tech::Fletching,
        Tech::PaddedArcherArmor => sim::Tech::PaddedArcherArmor,
        Tech::CastleAge => sim::Tech::CastleAge,
        Tech::LongSwordsman => sim::Tech::LongSwordsman,
        Tech::Pikeman => sim::Tech::Pikeman,
        Tech::Crossbowman => sim::Tech::Crossbowman,
        Tech::EliteSkirmisher => sim::Tech::EliteSkirmisher,
        Tech::LightCavalry => sim::Tech::LightCavalry,
        Tech::HandCart => sim::Tech::HandCart,
        Tech::TownPatrol => sim::Tech::TownPatrol,
        Tech::IronCasting => sim::Tech::IronCasting,
        Tech::ChainMailArmor => sim::Tech::ChainMailArmor,
        Tech::ChainBardingArmor => sim::Tech::ChainBardingArmor,
        Tech::BodkinArrow => sim::Tech::BodkinArrow,
        Tech::LeatherArcherArmor => sim::Tech::LeatherArcherArmor,
        Tech::HeavyPlow => sim::Tech::HeavyPlow,
        Tech::BowSaw => sim::Tech::BowSaw,
        Tech::GoldShaftMining => sim::Tech::GoldShaftMining,
        Tech::Masonry => sim::Tech::Masonry,
        Tech::Ballistics => sim::Tech::Ballistics,
        Tech::MurderHoles => sim::Tech::MurderHoles,
        Tech::ImperialAge => sim::Tech::ImperialAge,
        Tech::TwoHandedSwordsman => sim::Tech::TwoHandedSwordsman,
        Tech::Champion => sim::Tech::Champion,
        Tech::Halberdier => sim::Tech::Halberdier,
        Tech::Arbalester => sim::Tech::Arbalester,
        Tech::Cavalier => sim::Tech::Cavalier,
        Tech::Paladin => sim::Tech::Paladin,
        Tech::Hussar => sim::Tech::Hussar,
        Tech::CappedRam => sim::Tech::CappedRam,
        Tech::SiegeRam => sim::Tech::SiegeRam,
        Tech::Onager => sim::Tech::Onager,
        Tech::Chemistry => sim::Tech::Chemistry,
        Tech::HeavyCavalryArcher => sim::Tech::HeavyCavalryArcher,
        Tech::HeavyCamelRider => sim::Tech::HeavyCamelRider,
        Tech::HeavyScorpion => sim::Tech::HeavyScorpion,
        Tech::SiegeOnager => sim::Tech::SiegeOnager,
        Tech::BlastFurnace => sim::Tech::BlastFurnace,
        Tech::PlateMailArmor => sim::Tech::PlateMailArmor,
        Tech::PlateBardingArmor => sim::Tech::PlateBardingArmor,
        Tech::Bracer => sim::Tech::Bracer,
        Tech::RingArcherArmor => sim::Tech::RingArcherArmor,
        Tech::CropRotation => sim::Tech::CropRotation,
        Tech::TwoManSaw => sim::Tech::TwoManSaw,
        Tech::StoneShaftMining => sim::Tech::StoneShaftMining,
        Tech::Squires => sim::Tech::Squires,
        Tech::Arson => sim::Tech::Arson,
        Tech::Gambesons => sim::Tech::Gambesons,
        Tech::ThumbRing => sim::Tech::ThumbRing,
        Tech::ParthianTactics => sim::Tech::ParthianTactics,
        Tech::Bloodlines => sim::Tech::Bloodlines,
        Tech::Husbandry => sim::Tech::Husbandry,
        Tech::Architecture => sim::Tech::Architecture,
        Tech::SiegeEngineers => sim::Tech::SiegeEngineers,
        Tech::HeatedShot => sim::Tech::HeatedShot,
        Tech::Arrowslits => sim::Tech::Arrowslits,
        Tech::FortifiedWall => sim::Tech::FortifiedWall,
        Tech::GuardTower => sim::Tech::GuardTower,
        Tech::Keep => sim::Tech::Keep,
        Tech::Devotion => sim::Tech::Devotion,
        Tech::Faith => sim::Tech::Faith,
        Tech::Fervor => sim::Tech::Fervor,
        Tech::Sanctity => sim::Tech::Sanctity,
        Tech::HerbalMedicine => sim::Tech::HerbalMedicine,
        Tech::Theocracy => sim::Tech::Theocracy,
        Tech::Illumination => sim::Tech::Illumination,
        Tech::BlockPrinting => sim::Tech::BlockPrinting,
        Tech::Atonement => sim::Tech::Atonement,
        Tech::Redemption => sim::Tech::Redemption,
        Tech::Heresy => sim::Tech::Heresy,
        Tech::Conscription => sim::Tech::Conscription,
        Tech::Hoardings => sim::Tech::Hoardings,
        Tech::Sappers => sim::Tech::Sappers,
        Tech::Spies => sim::Tech::Spies,
        Tech::Caravan => sim::Tech::Caravan,
        Tech::Guilds => sim::Tech::Guilds,
        Tech::Banking => sim::Tech::Banking,
        Tech::Coinage => sim::Tech::Coinage,
        Tech::TreadmillCrane => sim::Tech::TreadmillCrane,
        Tech::WarGalley => sim::Tech::WarGalley,
    }
}

/// Placement cost: per segment for palisades, per water tile for bridges.
pub fn building_cost(kind: BuildingKind) -> Stockpile {
    stock(native_building(kind).rules().cost)
}

/// One-villager build seconds (per water tile for a bridge).
pub fn building_build_seconds(kind: BuildingKind) -> u32 {
    native_building(kind).rules().build_seconds
}

/// Population housed by a completed building of this kind.
pub fn building_population(kind: BuildingKind) -> u16 {
    native_building(kind).rules().population_support
}

/// The building that must already stand for this kind to be buildable.
pub fn building_requires(kind: BuildingKind) -> Option<BuildingKind> {
    native_building(kind).rules().requires.map(|required| {
        BuildingKind::ALL
            .into_iter()
            .find(|candidate| native_building(*candidate) == required)
            .unwrap_or(BuildingKind::TownCenter)
    })
}

/// Whether villagers can construct this building at all.
pub fn building_buildable(kind: BuildingKind) -> bool {
    native_building(kind).rules().buildable
}

/// Earliest age in which villagers may construct this kind.
pub fn building_required_age(kind: BuildingKind) -> crate::Age {
    match native_building(kind).rules().age {
        sim::Age::Dark => crate::Age::Dark,
        sim::Age::Feudal => crate::Age::Feudal,
        sim::Age::Castle => crate::Age::Castle,
        sim::Age::Imperial => crate::Age::Imperial,
    }
}

/// Units a completed building of this kind can train.
pub fn trainable(building: BuildingKind) -> Vec<UnitKind> {
    sim::trainable_units(native_building(building))
        .iter()
        .filter_map(|unit| UnitKind::ALL.into_iter().find(|k| native_unit(*k) == *unit))
        .collect()
}

/// What queueing a unit costs.
pub fn unit_cost(kind: UnitKind) -> Stockpile {
    match kind {
        UnitKind::Villager => Stockpile {
            food: sim::VILLAGE_RULES.villager_food_cost,
            ..Stockpile::default()
        },
        _ => sim::GameWorld::unit_training(native_unit(kind))
            .map(|training| Stockpile {
                food: training.cost.food,
                wood: training.cost.wood,
                gold: training.cost.gold,
                stone: training.cost.stone,
            })
            .unwrap_or_default(),
    }
}

/// Seconds a unit takes to train.
pub fn unit_train_seconds(kind: UnitKind) -> u32 {
    match kind {
        UnitKind::Villager => sim::VILLAGE_RULES.villager_training_seconds,
        _ => sim::GameWorld::unit_training(native_unit(kind))
            .map(|training| training.seconds)
            .unwrap_or(0),
    }
}

/// What researching a technology costs.
pub fn tech_cost(tech: Tech) -> Stockpile {
    let native = native_tech(tech);
    sim::TECH_RULES
        .iter()
        .find(|rules| rules.tech == native)
        .map(|rules| stock(rules.cost))
        .unwrap_or_default()
}

/// Villager carrying capacity before a trip to a drop-off.
pub fn villager_capacity() -> u16 {
    sim::VILLAGE_RULES.villager_capacity
}

/// Population each house (or town centre) supports in the base rules.
pub fn population_per_house() -> u16 {
    sim::BuildingKind::House.rules().population_support
}

/// One entry of the current public technology tree, including prerequisites.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Technology {
    pub tech: Tech,
    pub building: BuildingKind,
    pub age: crate::Age,
    pub prerequisite: Option<Tech>,
    pub building_count: usize,
    pub building_kinds: Vec<BuildingKind>,
    /// Base line identities which this technology upgrades.
    pub upgrades: Vec<UnitKind>,
    /// Unit identities requiring this technology to train.
    pub unlocks: Vec<UnitKind>,
    /// Units directly affected by a scoped speed or hit-point bonus.
    pub improves: Vec<UnitKind>,
    /// Improves resource collection, farms or villager carrying capacity.
    pub economic: bool,
}

pub fn technologies() -> Vec<Technology> {
    sim::TECH_RULES
        .iter()
        .filter_map(|row| {
            let tech = Tech::ALL
                .into_iter()
                .find(|t| native_tech(*t) == row.tech)?;
            let building = BuildingKind::ALL
                .into_iter()
                .find(|b| native_building(*b) == row.building)?;
            let (upgrades, unlocks, improves) = production::technology_units(row);
            let mut result = Technology {
                tech,
                building,
                age: public_age(row.age),
                prerequisite: None,
                building_count: 0,
                building_kinds: Vec::new(),
                upgrades,
                unlocks,
                improves,
                economic: matches!(
                    row.effect,
                    sim::TechEffect::GatherRate { .. }
                        | sim::TechEffect::FarmFood(_)
                        | sim::TechEffect::FarmFoodAndCarry { .. }
                        | sim::TechEffect::VillagerEconomy { .. }
                ),
            };
            match row.requires {
                sim::TechRequirement::Technology(required) => {
                    result.prerequisite =
                        Tech::ALL.into_iter().find(|t| native_tech(*t) == required);
                }
                sim::TechRequirement::BuildingKinds { count, kinds } => {
                    result.building_count = count;
                    result.building_kinds = BuildingKind::ALL
                        .into_iter()
                        .filter(|b| kinds.contains(&native_building(*b)))
                        .collect();
                }
                sim::TechRequirement::Age(age) => result.age = public_age(age),
                sim::TechRequirement::None => {}
            }
            Some(result)
        })
        .collect()
}

/// Gold charged or paid for one ordinary 100-unit market trade.
pub fn market_quote(price: u16, buy: bool) -> u32 {
    if buy {
        sim::MARKET_RULES.buy_cost(price)
    } else {
        sim::MARKET_RULES.sell_proceeds(price)
    }
}

fn public_age(age: sim::Age) -> crate::Age {
    match age {
        sim::Age::Dark => crate::Age::Dark,
        sim::Age::Feudal => crate::Age::Feudal,
        sim::Age::Castle => crate::Age::Castle,
        sim::Age::Imperial => crate::Age::Imperial,
    }
}
