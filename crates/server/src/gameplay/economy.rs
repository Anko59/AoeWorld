use super::GameplayService;
use aoe_core::{EntityId, PlayerId, TileRect};
use aoe_protocol::{
    Age, BuildingKind, CarriedResource, ConstructionState, GameplayBuildingState,
    GameplayPlayerState, GameplayUnitState, HitState, MAX_SUBSCRIBED_UNITS, ResearchState,
    ResourceKind, Stockpile, Tech, TradeResource, TrainingState, UnitKind, UnitTask,
};
use aoe_simulation::{GameUnit, GameWorld};
use std::collections::BTreeMap;

/// Economy units stay authoritative while offscreen, for selection and worker
/// counts. Their population is bounded by the village rules; population-only
/// workloads continue using the spatial query and the primary scout.
pub(super) fn query_units(
    world: &GameWorld,
    region: TileRect,
    primary_id: EntityId,
    player: PlayerId,
) -> Vec<GameUnit> {
    let mut units = world.query(region).0;
    if world.player_economy(player).is_some() {
        units.extend(
            world
                .units()
                .filter(|unit| unit.player == player)
                .take(MAX_SUBSCRIBED_UNITS + 1),
        );
    } else if let Some(primary) = world.unit(primary_id) {
        units.push(primary);
    }
    units.sort_unstable_by_key(|unit| unit.id);
    units.dedup_by_key(|unit| unit.id);
    units.retain(|unit| world.unit_visible_to(player, unit.id));
    units
}

pub(super) fn native_kind(kind: UnitKind) -> aoe_simulation::UnitKind {
    match kind {
        UnitKind::Villager => aoe_simulation::UnitKind::Villager,
        UnitKind::Scout => aoe_simulation::UnitKind::Scout,
        UnitKind::Militia => aoe_simulation::UnitKind::Militia,
        UnitKind::FishingShip => aoe_simulation::UnitKind::FishingShip,
        UnitKind::Archer => aoe_simulation::UnitKind::Archer,
        UnitKind::Skirmisher => aoe_simulation::UnitKind::Skirmisher,
        UnitKind::Spearman => aoe_simulation::UnitKind::Spearman,
        UnitKind::ManAtArms => aoe_simulation::UnitKind::ManAtArms,
    }
}
pub(super) fn native_tech(tech: Tech) -> aoe_simulation::Tech {
    match tech {
        Tech::Loom => aoe_simulation::Tech::Loom,
        Tech::TownWatch => aoe_simulation::Tech::TownWatch,
        Tech::DoubleBitAxe => aoe_simulation::Tech::DoubleBitAxe,
        Tech::HorseCollar => aoe_simulation::Tech::HorseCollar,
        Tech::GoldMining => aoe_simulation::Tech::GoldMining,
        Tech::StoneMining => aoe_simulation::Tech::StoneMining,
        Tech::Wheelbarrow => aoe_simulation::Tech::Wheelbarrow,
        Tech::FeudalAge => aoe_simulation::Tech::FeudalAge,
        Tech::ManAtArms => aoe_simulation::Tech::ManAtArms,
        Tech::Forging => aoe_simulation::Tech::Forging,
        Tech::ScaleMailArmor => aoe_simulation::Tech::ScaleMailArmor,
        Tech::ScaleBardingArmor => aoe_simulation::Tech::ScaleBardingArmor,
        Tech::Fletching => aoe_simulation::Tech::Fletching,
        Tech::PaddedArcherArmor => aoe_simulation::Tech::PaddedArcherArmor,
    }
}
fn wire_tech(tech: aoe_simulation::Tech) -> Tech {
    match tech {
        aoe_simulation::Tech::Loom => Tech::Loom,
        aoe_simulation::Tech::TownWatch => Tech::TownWatch,
        aoe_simulation::Tech::DoubleBitAxe => Tech::DoubleBitAxe,
        aoe_simulation::Tech::HorseCollar => Tech::HorseCollar,
        aoe_simulation::Tech::GoldMining => Tech::GoldMining,
        aoe_simulation::Tech::StoneMining => Tech::StoneMining,
        aoe_simulation::Tech::Wheelbarrow => Tech::Wheelbarrow,
        aoe_simulation::Tech::FeudalAge => Tech::FeudalAge,
        aoe_simulation::Tech::ManAtArms => Tech::ManAtArms,
        aoe_simulation::Tech::Forging => Tech::Forging,
        aoe_simulation::Tech::ScaleMailArmor => Tech::ScaleMailArmor,
        aoe_simulation::Tech::ScaleBardingArmor => Tech::ScaleBardingArmor,
        aoe_simulation::Tech::Fletching => Tech::Fletching,
        aoe_simulation::Tech::PaddedArcherArmor => Tech::PaddedArcherArmor,
    }
}
pub(super) fn native_building(kind: BuildingKind) -> aoe_simulation::BuildingKind {
    use aoe_simulation::BuildingKind as N;
    match kind {
        BuildingKind::TownCenter => N::TownCenter,
        BuildingKind::House => N::House,
        BuildingKind::LumberCamp => N::LumberCamp,
        BuildingKind::MiningCamp => N::MiningCamp,
        BuildingKind::Mill => N::Mill,
        BuildingKind::Farm => N::Farm,
        BuildingKind::Barracks => N::Barracks,
        BuildingKind::PalisadeWall => N::PalisadeWall,
        BuildingKind::ArcheryRange => N::ArcheryRange,
        BuildingKind::Stable => N::Stable,
        BuildingKind::Blacksmith => N::Blacksmith,
        BuildingKind::Market => N::Market,
        BuildingKind::WatchTower => N::WatchTower,
        BuildingKind::PalisadeGate => N::PalisadeGate,
        BuildingKind::Outpost => N::Outpost,
        BuildingKind::Dock => N::Dock,
    }
}
fn wire_building(kind: aoe_simulation::BuildingKind) -> BuildingKind {
    use aoe_simulation::BuildingKind as N;
    match kind {
        N::TownCenter => BuildingKind::TownCenter,
        N::House => BuildingKind::House,
        N::LumberCamp => BuildingKind::LumberCamp,
        N::MiningCamp => BuildingKind::MiningCamp,
        N::Mill => BuildingKind::Mill,
        N::Farm => BuildingKind::Farm,
        N::Barracks => BuildingKind::Barracks,
        N::PalisadeWall => BuildingKind::PalisadeWall,
        N::ArcheryRange => BuildingKind::ArcheryRange,
        N::Stable => BuildingKind::Stable,
        N::Blacksmith => BuildingKind::Blacksmith,
        N::Market => BuildingKind::Market,
        N::WatchTower => BuildingKind::WatchTower,
        N::PalisadeGate => BuildingKind::PalisadeGate,
        N::Outpost => BuildingKind::Outpost,
        N::Dock => BuildingKind::Dock,
    }
}
fn wire_kind(kind: aoe_simulation::UnitKind) -> UnitKind {
    match kind {
        aoe_simulation::UnitKind::Villager => UnitKind::Villager,
        aoe_simulation::UnitKind::Scout => UnitKind::Scout,
        aoe_simulation::UnitKind::Militia => UnitKind::Militia,
        aoe_simulation::UnitKind::FishingShip => UnitKind::FishingShip,
        aoe_simulation::UnitKind::Archer => UnitKind::Archer,
        aoe_simulation::UnitKind::Skirmisher => UnitKind::Skirmisher,
        aoe_simulation::UnitKind::Spearman => UnitKind::Spearman,
        aoe_simulation::UnitKind::ManAtArms => UnitKind::ManAtArms,
    }
}
fn wire_hit(hit: aoe_simulation::HitRecord) -> HitState {
    HitState {
        attacker: hit.attacker,
        tick: hit.tick,
    }
}
fn wire_resource(kind: aoe_map::ResourceKind) -> ResourceKind {
    match kind {
        aoe_map::ResourceKind::Food => ResourceKind::Food,
        aoe_map::ResourceKind::Wood => ResourceKind::Wood,
        aoe_map::ResourceKind::Gold => ResourceKind::Gold,
        aoe_map::ResourceKind::Stone => ResourceKind::Stone,
    }
}
pub(super) fn unit_state(world: &GameWorld, unit: &GameUnit) -> GameplayUnitState {
    wire_unit(
        unit,
        world.gathering_kind(unit.id),
        world.hunting_target(unit.id),
    )
}
/// A unit as the wire sees it, given the two facts only the economy knows.
pub(super) fn wire_unit(
    unit: &GameUnit,
    gathering: Option<aoe_map::ResourceKind>,
    hunt_target: Option<EntityId>,
) -> GameplayUnitState {
    GameplayUnitState {
        id: unit.id,
        player: unit.player,
        position: unit.position,
        moving: unit.moving,
        planning: unit.planning,
        facing: unit.facing as u8,
        kind: wire_kind(unit.kind),
        task: match unit.task {
            aoe_simulation::UnitTask::Idle => UnitTask::Idle,
            aoe_simulation::UnitTask::Moving => UnitTask::Moving,
            aoe_simulation::UnitTask::Gathering => UnitTask::Gathering,
            aoe_simulation::UnitTask::Carrying => UnitTask::Carrying,
            aoe_simulation::UnitTask::Returning => UnitTask::Returning,
            aoe_simulation::UnitTask::Building => UnitTask::Building,
            aoe_simulation::UnitTask::Farming => UnitTask::Farming,
            aoe_simulation::UnitTask::Attacking => UnitTask::Attacking,
            aoe_simulation::UnitTask::Dying => UnitTask::Dying,
            aoe_simulation::UnitTask::Hunting => UnitTask::Hunting,
            aoe_simulation::UnitTask::Shepherding => UnitTask::Shepherding,
            aoe_simulation::UnitTask::Fishing => UnitTask::Fishing,
        },
        gathering: gathering.map(wire_resource),
        hp: unit.hp,
        max_hp: unit.max_hp,
        attack_target: unit.attack_target,
        last_hit: unit.last_hit.map(wire_hit),
        death_tick: unit.death_tick,
        hunt_target,
        auto_scout: unit.auto_scout,
        death_hunt: unit.death_hunt.map(|kind| match kind {
            aoe_simulation::AnimalKind::Sheep => aoe_protocol::AnimalKind::Sheep,
            aoe_simulation::AnimalKind::Deer => aoe_protocol::AnimalKind::Deer,
            aoe_simulation::AnimalKind::Boar => aoe_protocol::AnimalKind::Boar,
            aoe_simulation::AnimalKind::Wolf => aoe_protocol::AnimalKind::Wolf,
        }),
        carried: unit.carried.map(|load| CarriedResource {
            kind: wire_resource(load.kind),
            amount: load.amount,
        }),
    }
}
pub(super) fn native_trade(resource: TradeResource) -> aoe_map::ResourceKind {
    match resource {
        TradeResource::Wood => aoe_map::ResourceKind::Wood,
        TradeResource::Food => aoe_map::ResourceKind::Food,
        TradeResource::Stone => aoe_map::ResourceKind::Stone,
    }
}
fn wire_market_prices(prices: aoe_simulation::MarketPrices) -> aoe_protocol::MarketPrices {
    aoe_protocol::MarketPrices {
        wood: prices.wood,
        food: prices.food,
        stone: prices.stone,
    }
}
pub(super) fn players(world: &GameWorld) -> BTreeMap<PlayerId, GameplayPlayerState> {
    world
        .players()
        .map(|state| (state.player, player_state(&state)))
        .collect()
}
pub(super) fn player(world: &GameWorld, player: PlayerId) -> Option<GameplayPlayerState> {
    world
        .player_economy(player)
        .map(|state| player_state(&state))
}
fn player_state(state: &aoe_simulation::PlayerEconomy) -> GameplayPlayerState {
    GameplayPlayerState {
        player: state.player,
        stockpile: Stockpile {
            food: state.stockpile.food,
            wood: state.stockpile.wood,
            gold: state.stockpile.gold,
            stone: state.stockpile.stone,
        },
        population_used: state.population_used,
        population_cap: state.population_cap,
        age: match state.age {
            aoe_simulation::Age::Dark => Age::Dark,
            aoe_simulation::Age::Feudal => Age::Feudal,
        },
        researched: state.researched,
        market_prices: wire_market_prices(state.market_prices),
    }
}
pub(super) fn buildings(
    world: &GameWorld,
    player: PlayerId,
) -> BTreeMap<EntityId, GameplayBuildingState> {
    world
        .known_buildings(player)
        .iter()
        .map(|state| {
            (
                state.id,
                wire_building_state(state, (!world.fog_disabled()).then_some(player)),
            )
        })
        .collect()
}
pub(super) fn wire_building_state(
    state: &aoe_simulation::GameBuilding,
    viewer: Option<PlayerId>,
) -> GameplayBuildingState {
    GameplayBuildingState {
        age: match state.age {
            aoe_simulation::Age::Dark => aoe_protocol::Age::Dark,
            aoe_simulation::Age::Feudal => aoe_protocol::Age::Feudal,
        },
        id: state.id,
        gate_open: state.gate_open,
        rally_point: state
            .rally_point
            .filter(|_| viewer.is_none_or(|player| state.player == player))
            .map(|r| aoe_protocol::RallyPoint {
                position: r.position,
                target: wire_rally(r.target),
            }),
        player: state.player,
        kind: wire_building(state.kind),
        footprint: state.footprint,
        construction: match state.state {
            aoe_simulation::ConstructionState::UnderConstruction => {
                ConstructionState::UnderConstruction
            }
            aoe_simulation::ConstructionState::Complete => ConstructionState::Complete,
        },
        progress_permille: state.progress_permille(),
        hp: state.hp,
        max_hp: state.max_hp,
        los: state.los,
        food_remaining: state.food_remaining,
        farm_reseeds: state.farm_reseeds,
        last_hit: state.last_hit.map(wire_hit),
        research: state.research.map(|entry| ResearchState {
            tech: wire_tech(entry.tech),
            progress_ticks: entry.progress_ticks,
            total_ticks: entry.total_ticks,
        }),
        training_queue: state
            .training_queue
            .iter()
            .map(|entry| TrainingState {
                kind: wire_kind(entry.kind),
                progress_ticks: entry.progress_ticks,
                total_ticks: entry.total_ticks,
            })
            .collect(),
    }
}

impl GameplayService {
    pub(super) async fn advance_world(
        &self,
        mut world: tokio::sync::OwnedRwLockWriteGuard<GameWorld>,
    ) -> Option<tokio::sync::OwnedRwLockWriteGuard<GameWorld>> {
        let hash = self.map_content_hash;
        let directory = self.resource_directory.clone();
        let journal = self.resource_journal.clone();
        let probe = self.probe.clone();
        let advance = move || {
            let mut phase_started = std::time::Instant::now();
            world.advance_with_probe(
                &mut |transaction| {
                    if let (Some(hash), Some(directory)) = (hash, &directory) {
                        match super::resources::save_gathering(
                            directory,
                            &transaction.snapshot(hash),
                            transaction.previous_revision(),
                        ) {
                            Ok(_) => true,
                            Err(error) => {
                                tracing::warn!("gathering persistence refused: {error}");
                                false
                            }
                        }
                    } else {
                        true
                    }
                },
                &mut |phase| {
                    let now = std::time::Instant::now();
                    let name = match phase {
                        aoe_simulation::TickPhase::Movement => "movement",
                        aoe_simulation::TickPhase::Spacing => "spacing",
                        aoe_simulation::TickPhase::Wildlife => "wildlife",
                        aoe_simulation::TickPhase::Combat => "combat",
                        aoe_simulation::TickPhase::Construction => "construction",
                        aoe_simulation::TickPhase::Gathering => "gathering",
                        aoe_simulation::TickPhase::ResearchTraining => "research_training",
                        aoe_simulation::TickPhase::BuildingMemory => "building_memory",
                        aoe_simulation::TickPhase::Changed => "changed",
                    };
                    probe.phase(name, now.duration_since(phase_started));
                    phase_started = now;
                },
            );
            let journal_started = std::time::Instant::now();
            let changes = world.take_resource_changes();
            let mut journal = journal.lock().unwrap_or_else(|error| error.into_inner());
            for change in changes {
                journal.record(change);
            }
            drop(journal);
            probe.phase("resource_journal", journal_started.elapsed());
            world
        };
        if self.resource_directory.is_some() || self.map_content_hash.is_some() {
            tokio::task::spawn_blocking(advance).await.ok()
        } else {
            Some(advance())
        }
    }
}

/// Transport conversion stays at the adapter boundary.
pub(super) fn native_rally(target: aoe_protocol::RallyTarget) -> aoe_simulation::RallyTarget {
    use aoe_protocol::RallyTarget as W;
    use aoe_simulation::RallyTarget as N;
    match target {
        W::Ground(p) => N::Ground(p),
        W::Resource(id) => N::Resource(id),
        W::Building(id) => N::Building(id),
        W::Animal(id) => N::Animal(id),
    }
}
fn wire_rally(target: aoe_simulation::RallyTarget) -> aoe_protocol::RallyTarget {
    use aoe_protocol::RallyTarget as W;
    use aoe_simulation::RallyTarget as N;
    match target {
        N::Ground(p) => W::Ground(p),
        N::Resource(id) => W::Resource(id),
        N::Building(id) => W::Building(id),
        N::Animal(id) => W::Animal(id),
    }
}
