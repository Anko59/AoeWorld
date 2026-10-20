use super::GameplayService;
use aoe_core::{EntityId, PlayerId, TileRect};
use aoe_protocol::{
    Age, BuildingKind, CarriedResource, ConstructionState, GameplayBuildingState,
    GameplayPlayerState, GameplayUnitState, HitState, MAX_SUBSCRIBED_UNITS, ResearchState,
    ResourceKind, Stockpile, Tech, TrainingState, UnitKind, UnitTask,
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
) -> Vec<GameUnit> {
    let mut units = world.query(region).0;
    if let Some(primary) = world.unit(primary_id) {
        if world.player_economy(primary.player).is_some() {
            units.extend(
                world
                    .units()
                    .filter(|unit| unit.player == primary.player)
                    .take(MAX_SUBSCRIBED_UNITS + 1),
            );
        } else {
            units.push(primary);
        }
    }
    units.sort_unstable_by_key(|unit| unit.id);
    units.dedup_by_key(|unit| unit.id);
    units
}

pub(super) fn native_kind(kind: UnitKind) -> aoe_simulation::UnitKind {
    match kind {
        UnitKind::Villager => aoe_simulation::UnitKind::Villager,
        UnitKind::Scout => aoe_simulation::UnitKind::Scout,
        UnitKind::Militia => aoe_simulation::UnitKind::Militia,
    }
}
pub(super) fn native_tech(tech: Tech) -> aoe_simulation::Tech {
    match tech {
        Tech::Loom => aoe_simulation::Tech::Loom,
        Tech::TownWatch => aoe_simulation::Tech::TownWatch,
        Tech::FeudalAge => aoe_simulation::Tech::FeudalAge,
    }
}
fn wire_tech(tech: aoe_simulation::Tech) -> Tech {
    match tech {
        aoe_simulation::Tech::Loom => Tech::Loom,
        aoe_simulation::Tech::TownWatch => Tech::TownWatch,
        aoe_simulation::Tech::FeudalAge => Tech::FeudalAge,
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
        BuildingKind::Outpost => N::Outpost,
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
        N::Outpost => BuildingKind::Outpost,
    }
}
fn wire_kind(kind: aoe_simulation::UnitKind) -> UnitKind {
    match kind {
        aoe_simulation::UnitKind::Villager => UnitKind::Villager,
        aoe_simulation::UnitKind::Scout => UnitKind::Scout,
        aoe_simulation::UnitKind::Militia => UnitKind::Militia,
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
        },
        gathering: world.gathering_kind(unit.id).map(wire_resource),
        hp: unit.hp,
        max_hp: unit.max_hp,
        attack_target: unit.attack_target,
        last_hit: unit.last_hit.map(wire_hit),
        death_tick: unit.death_tick,
        hunt_target: world.hunting_target(unit.id),
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
pub(super) fn players(world: &GameWorld) -> BTreeMap<PlayerId, GameplayPlayerState> {
    world
        .players()
        .map(|state| {
            (
                state.player,
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
                },
            )
        })
        .collect()
}
pub(super) fn buildings(world: &GameWorld) -> BTreeMap<EntityId, GameplayBuildingState> {
    world
        .buildings()
        .map(|state| {
            (
                state.id,
                GameplayBuildingState {
                    id: state.id,
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
                },
            )
        })
        .collect()
}

impl GameplayService {
    pub(super) async fn advance_world(
        &self,
        mut world: tokio::sync::OwnedRwLockWriteGuard<GameWorld>,
    ) -> Option<tokio::sync::OwnedRwLockWriteGuard<GameWorld>> {
        let hash = self.map_content_hash;
        let directory = self.resource_directory.clone();
        let journal = self.resource_journal.clone();
        let advance = move || {
            world.advance_with_resource_sink(&mut |transaction| {
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
            });
            let changes = world.take_resource_changes();
            let mut journal = journal.lock().unwrap_or_else(|error| error.into_inner());
            for change in changes {
                journal.record(change);
            }
            drop(journal);
            world
        };
        if self.resource_directory.is_some() {
            tokio::task::spawn_blocking(advance).await.ok()
        } else {
            Some(advance())
        }
    }
}
