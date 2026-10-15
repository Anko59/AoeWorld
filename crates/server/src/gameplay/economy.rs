use super::GameplayService;
use aoe_core::{EntityId, PlayerId};
use aoe_protocol::{
    BuildingKind, CarriedResource, GameplayBuildingState, GameplayPlayerState, GameplayUnitState,
    ResourceKind, Stockpile, TrainingState, UnitKind, UnitTask,
};
use aoe_simulation::{GameUnit, GameWorld};
use std::collections::BTreeMap;

pub(super) fn native_kind(kind: UnitKind) -> aoe_simulation::UnitKind {
    match kind {
        UnitKind::Villager => aoe_simulation::UnitKind::Villager,
        UnitKind::Scout => aoe_simulation::UnitKind::Scout,
    }
}
fn wire_kind(kind: aoe_simulation::UnitKind) -> UnitKind {
    match kind {
        aoe_simulation::UnitKind::Villager => UnitKind::Villager,
        aoe_simulation::UnitKind::Scout => UnitKind::Scout,
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
pub(super) fn unit_state(unit: &GameUnit) -> GameplayUnitState {
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
        },
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
                    kind: match state.kind {
                        aoe_simulation::BuildingKind::TownCenter => BuildingKind::TownCenter,
                    },
                    footprint: state.footprint,
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
