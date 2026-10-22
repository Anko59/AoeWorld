//! The game's public rules tables (costs and times), so an AI can plan without
//! trial and error. They are the same tables the server charges from.
use crate::model::{BuildingKind, Stockpile, Tech, UnitKind};
use aoe_simulation as sim;

fn stock(cost: sim::Stockpile) -> Stockpile {
    Stockpile {
        food: cost.food,
        wood: cost.wood,
        gold: cost.gold,
        stone: cost.stone,
    }
}

fn native_building(kind: BuildingKind) -> sim::BuildingKind {
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
        BuildingKind::PalisadeGate => N::PalisadeGate,
    }
}

fn native_unit(kind: UnitKind) -> sim::UnitKind {
    match kind {
        UnitKind::Villager => sim::UnitKind::Villager,
        UnitKind::Scout => sim::UnitKind::Scout,
        UnitKind::Militia => sim::UnitKind::Militia,
        UnitKind::FishingShip => sim::UnitKind::FishingShip,
        UnitKind::Archer => sim::UnitKind::Archer,
        UnitKind::Skirmisher => sim::UnitKind::Skirmisher,
        UnitKind::Spearman => sim::UnitKind::Spearman,
        UnitKind::ManAtArms => sim::UnitKind::ManAtArms,
    }
}

fn native_tech(tech: Tech) -> sim::Tech {
    match tech {
        Tech::Loom => sim::Tech::Loom,
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
    }
}

/// What placing a building costs (per segment for palisades).
pub fn building_cost(kind: BuildingKind) -> Stockpile {
    stock(native_building(kind).rules().cost)
}

/// Seconds one villager takes to build it, as the rules table states.
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
    sim::VILLAGE_RULES.population_support
}
