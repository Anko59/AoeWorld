//! Mutable economy indexes and test-only work accounting.
use super::{
    BuildTask, BuildingKind, GameBuilding, GarrisonedUnit, PlayerEconomy, WorldLimits, farm,
    gathering, scouting, vassal,
};
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};
use std::collections::{BTreeMap, BTreeSet};

#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct TickWork {
    pub(crate) training_buildings: usize,
    pub(crate) research_buildings: usize,
    pub(crate) spawn_units: usize,
}

#[derive(Debug, Default)]
pub(crate) struct Economy {
    pub(crate) vassals: vassal::Manager,
    pub(crate) vassal_homes: BTreeMap<EntityId, vassal::Home>,
    #[cfg(test)]
    pub(crate) tick_work: TickWork,
    pub(crate) garrison_orders: BTreeMap<EntityId, EntityId>,
    pub(crate) garrison_reservations: BTreeMap<EntityId, u8>,
    pub(crate) occupied_garrisons: BTreeSet<EntityId>,
    pub(crate) ejections: BTreeMap<EntityId, (TileRect, Vec<GarrisonedUnit>)>,
    pub(crate) cargo: BTreeMap<EntityId, Vec<EntityId>>,
    pub(crate) transport_journeys: BTreeMap<EntityId, super::super::navy::Journey>,
    pub(crate) embarked: BTreeMap<EntityId, EntityId>,
    pub(crate) players: BTreeMap<PlayerId, PlayerEconomy>,
    pub(crate) buildings: BTreeMap<EntityId, GameBuilding>,
    pub(crate) farm_production: BTreeMap<EntityId, farm::FarmProduction>,
    // Derived ordered worksets; building queues remain the canonical state.
    pub(crate) active_training: BTreeSet<EntityId>,
    pub(crate) active_research: BTreeSet<EntityId>,
    /// Every building per player and kind, ascending by id: drop-off
    /// candidates, housing and requirements read one player's buildings
    /// without scanning the world. Maintained as buildings are placed and
    /// removed; completion is checked per query.
    pub(crate) dropoffs: BTreeMap<(PlayerId, BuildingKind), Vec<EntityId>>,
    pub(crate) blocked_tiles: BTreeSet<TileCoord>,
    pub(crate) open_gates: BTreeSet<EntityId>,
    pub(crate) scouts: BTreeMap<EntityId, scouting::AutoScout>,
    pub(crate) scout_searches: BTreeSet<EntityId>,
    pub(crate) scout_cursor: Option<EntityId>,
    pub(crate) gatherers: gathering::Gatherers,
    pub(crate) builders: BTreeMap<EntityId, BuildTask>,
    pub(crate) depletions: Vec<aoe_map::Depletion>,
    pub(crate) limits: WorldLimits,
}
