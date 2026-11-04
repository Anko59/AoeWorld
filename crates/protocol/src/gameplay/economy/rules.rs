//! Per-entity wire invariants. Each function names the first rule an entity
//! breaks, so a failing state is diagnosable and one bad entity can be repaired
//! or omitted without dropping the whole message.
use super::*;
use crate::gameplay::{RallyTarget, UnitState, automation, garrison};

fn valid_prices(prices: MarketPrices) -> bool {
    [prices.wood, prices.food, prices.stone]
        .iter()
        .all(|price| (MARKET_MIN_PRICE..=MARKET_MAX_PRICE).contains(price))
}
fn valid_research(player: &GameplayPlayerState) -> bool {
    let known = Tech::ALL.iter().fold(0, |bits, tech| bits | tech.bit());
    player.researched & !known == 0
        && (player.age >= Age::Feudal || player.researched & !Tech::Loom.bit() == 0)
        && (player.age >= Age::Feudal) == (player.researched & Tech::FeudalAge.bit() != 0)
        && (player.age >= Age::Castle) == (player.researched & Tech::CastleAge.bit() != 0)
        && (player.age == Age::Imperial) == (player.researched & Tech::ImperialAge.bit() != 0)
        && (player.age >= Age::Castle
            || player.researched & Tech::ALL[15..].iter().fold(0, |bits, t| bits | t.bit()) == 0)
        && (player.age == Age::Imperial
            || player.researched & Tech::ALL[34..].iter().fold(0, |bits, t| bits | t.bit()) == 0)
}

/// The first rule `player` breaks.
pub(crate) fn player_rule(player: &GameplayPlayerState) -> Option<&'static str> {
    if !valid_research(player) {
        return Some("player.researched_matches_age");
    }
    if !valid_prices(player.market_prices) {
        return Some("player.market_prices_in_range");
    }
    None
}

fn rally_rule(building: &GameplayBuildingState) -> Option<&'static str> {
    let r = building.rally_point?;
    let rejects = matches!(
        building.kind,
        BuildingKind::House
            | BuildingKind::Farm
            | BuildingKind::Mill
            | BuildingKind::LumberCamp
            | BuildingKind::MiningCamp
            | BuildingKind::Bridge
            | BuildingKind::PalisadeWall
            | BuildingKind::PalisadeGate
            | BuildingKind::StoneWall
            | BuildingKind::StoneGate
            | BuildingKind::Outpost
            | BuildingKind::Blacksmith
            | BuildingKind::Market
            | BuildingKind::WatchTower
            | BuildingKind::University
    );
    let ground_differs = matches!(r.target, RallyTarget::Ground(p) if p != r.position);
    (r.position.x < 0 || r.position.y < 0 || rejects || ground_differs)
        .then_some("building.rally_point")
}

/// Whether `building` may train `unit`.
fn trains(building: BuildingKind, unit: UnitKind) -> bool {
    matches!(
        (building, unit),
        (BuildingKind::Castle, UnitKind::Vassal | UnitKind::General)
            | (
                BuildingKind::TownCenter,
                UnitKind::Villager | UnitKind::Steward
            )
            | (BuildingKind::Barracks, UnitKind::Militia)
            | (
                BuildingKind::Barracks,
                UnitKind::Spearman
                    | UnitKind::ManAtArms
                    | UnitKind::LongSwordsman
                    | UnitKind::Pikeman
                    | UnitKind::TwoHandedSwordsman
                    | UnitKind::Champion
                    | UnitKind::Halberdier
            )
            | (
                BuildingKind::ArcheryRange,
                UnitKind::Archer
                    | UnitKind::Skirmisher
                    | UnitKind::Crossbowman
                    | UnitKind::EliteSkirmisher
                    | UnitKind::Arbalester
            )
            | (
                BuildingKind::Stable,
                UnitKind::Scout
                    | UnitKind::Knight
                    | UnitKind::LightCavalry
                    | UnitKind::Cavalier
                    | UnitKind::Paladin
                    | UnitKind::Hussar
            )
            | (BuildingKind::Monastery, UnitKind::Monk)
            | (
                BuildingKind::SiegeWorkshop,
                UnitKind::Mangonel
                    | UnitKind::BatteringRam
                    | UnitKind::CappedRam
                    | UnitKind::SiegeRam
                    | UnitKind::Onager
            )
            | (
                BuildingKind::Dock,
                UnitKind::FishingShip
                    | UnitKind::Galley
                    | UnitKind::FireShip
                    | UnitKind::DemolitionShip
                    | UnitKind::TransportShip
                    | UnitKind::WarGalley
            )
    )
}

/// The first rule `building` breaks.
pub(crate) fn building_rule(building: &GameplayBuildingState) -> Option<&'static str> {
    let complete = building.construction == ConstructionState::Complete;
    if !automation::valid_reseeds(building) {
        return Some("building.farm_reseeds");
    }
    if !garrison::valid(building) {
        return Some("building.garrison");
    }
    if let Some(rule) = rally_rule(building) {
        return Some(rule);
    }
    if building.footprint.min.x < 0 || building.footprint.min.y < 0 {
        return Some("building.footprint_non_negative");
    }
    if building.hp == 0 {
        return Some("building.hp_positive");
    }
    if building.gate_open
        && !(matches!(
            building.kind,
            BuildingKind::PalisadeGate | BuildingKind::StoneGate
        ) && complete)
    {
        return Some("building.gate_open");
    }
    let footprint_ok = if building.kind == BuildingKind::Bridge {
        building
            .bridge_span
            .is_some_and(|span| span.valid() && span.footprint() == building.footprint)
    } else {
        building.bridge_span.is_none()
            && building.footprint.width() == building.kind.side()
            && building.footprint.height() == building.kind.depth()
    };
    if !footprint_ok {
        return Some("building.footprint_matches_kind");
    }
    if building.progress_permille > 1000 || complete != (building.progress_permille == 1000) {
        return Some("building.progress_matches_construction");
    }
    if building.max_hp == 0 {
        return Some("building.max_hp_positive");
    }
    if building.hp > building.max_hp {
        return Some("building.hp_at_most_max_hp");
    }
    if (building.kind == BuildingKind::Farm) != building.food_remaining.is_some() {
        return Some("building.food_remaining_farm_only");
    }
    if !complete && !building.training_queue.is_empty() {
        return Some("building.queue_needs_complete");
    }
    if building.research.is_some_and(|research| {
        building.kind != research.tech.building()
            || !complete
            || !building.training_queue.is_empty()
            || research.total_ticks == 0
            || research.progress_ticks > research.total_ticks
    }) {
        return Some("building.research");
    }
    if building.training_queue.len() > MAX_TRAINING_QUEUE
        || building.training_queue.iter().any(|entry| {
            !trains(building.kind, entry.kind)
                || entry.total_ticks == 0
                || entry.progress_ticks > entry.total_ticks
        })
    {
        return Some("building.training_queue");
    }
    None
}

fn hunting_rule(unit: &UnitState) -> bool {
    !matches!(unit.task, UnitTask::Hunting | UnitTask::Shepherding)
        || (unit.kind == UnitKind::Villager
            && unit
                .carried
                .is_none_or(|load| load.kind == ResourceKind::Food)
            && unit.gathering.is_none_or(|kind| kind == ResourceKind::Food))
}
fn carried_rule(unit: &UnitState) -> bool {
    unit.carried.is_none_or(|load| {
        load.amount > 0
            && match unit.kind {
                // Hunters start at 35; DAT carry upgrades ceil to 67.
                // A stopped hunter keeps that food even without a job.
                UnitKind::Villager => {
                    let limit = if load.kind == ResourceKind::Food {
                        67
                    } else {
                        21
                    };
                    load.amount <= limit
                }
                UnitKind::FishingShip => load.kind == ResourceKind::Food && load.amount <= 15,
                _ => false,
            }
    })
}

/// The first rule `unit` breaks.
pub(crate) fn unit_rule(unit: &UnitState) -> Option<&'static str> {
    let alive = unit.hp > 0;
    if unit
        .home_castle
        .is_some_and(|id| !(unit.kind == UnitKind::General && alive && id != unit.id))
    {
        return Some("unit.home_castle");
    }
    if unit.managed_by.is_some_and(|id| {
        !(matches!(unit.kind, UnitKind::Villager | UnitKind::FishingShip) && alive && id != unit.id)
    }) {
        return Some("unit.managed_by");
    }
    if unit.commanded_by.is_some_and(|id| {
        id == unit.id
            || !alive
            || matches!(
                unit.kind,
                UnitKind::Villager
                    | UnitKind::Vassal
                    | UnitKind::Steward
                    | UnitKind::General
                    | UnitKind::Monk
            )
            || unit.kind.is_naval()
    }) {
        return Some("unit.commanded_by");
    }
    if unit.general.is_some_and(|g| {
        !(unit.kind == UnitKind::General
            && alive
            && g.army_size <= 200
            && g.mix.iter().all(|&share| share <= 100)
            && g.counts.iter().map(|&n| usize::from(n)).sum::<usize>() <= 200)
    }) {
        return Some("unit.general");
    }
    if unit.cargo_count > 5
        || (unit.cargo_count > 0 && !(unit.kind == UnitKind::TransportShip && alive))
    {
        return Some("unit.cargo_count");
    }
    if unit.facing >= 8 {
        return Some("unit.facing");
    }
    if !automation::valid_auto_scout(unit) {
        return Some("unit.auto_scout");
    }
    if unit.max_hp == 0 {
        return Some("unit.max_hp_positive");
    }
    if unit.hp > unit.max_hp {
        return Some("unit.hp_at_most_max_hp");
    }
    if unit.death_tick.is_some() && alive {
        return Some("unit.death_tick_only_when_dead");
    }
    if unit
        .death_hunt
        .is_some_and(|_| alive || unit.kind != UnitKind::Villager)
    {
        return Some("unit.death_hunt");
    }
    if unit
        .hunt_target
        .is_some_and(|id| unit.kind != UnitKind::Villager || id == unit.id)
    {
        return Some("unit.hunt_target");
    }
    if alive == (unit.task == UnitTask::Dying) {
        return Some("unit.dying_iff_no_hp");
    }
    if !alive && unit.attack_target.is_some() {
        return Some("unit.attack_target_when_dead");
    }
    if !hunting_rule(unit) {
        return Some("unit.hunting_task");
    }
    if unit.gathering.is_some_and(|kind| {
        !(unit.kind == UnitKind::Villager
            || (unit.kind == UnitKind::FishingShip && kind == ResourceKind::Food))
    }) {
        return Some("unit.gathering");
    }
    if !carried_rule(unit) {
        return Some("unit.carried");
    }
    if unit.task == UnitTask::Fishing
        && !(matches!(unit.kind, UnitKind::Villager | UnitKind::FishingShip)
            && unit.gathering == Some(ResourceKind::Food))
    {
        return Some("unit.fishing_task");
    }
    None
}
