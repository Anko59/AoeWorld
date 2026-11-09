//! Mutable economy indexes and test-only work accounting.
use super::{
    BuildTask, BuildingKind, GameBuilding, GarrisonedUnit, PlayerEconomy, WorldLimits, farm,
    gathering, scouting, steward,
};
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};
use std::collections::{BTreeMap, BTreeSet};
mod census;
mod income;

#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct TickWork {
    pub(crate) training_buildings: usize,
    pub(crate) research_buildings: usize,
    pub(crate) spawn_units: usize,
    /// Cumulative gatherer visits, and those that slept through the step.
    pub(crate) gather_visits: usize,
    pub(crate) gather_slept: usize,
}

#[derive(Debug, Default)]
pub(crate) struct Economy {
    pub(crate) land: std::sync::Mutex<super::land::Cache>,
    pub(crate) census: std::sync::Mutex<census::Cache>,
    pub(crate) vassals: super::vassal::Manager,
    pub(crate) generals: super::general::Manager,
    pub(crate) stewards: steward::Manager,
    pub(crate) steward_homes: BTreeMap<EntityId, steward::Home>,
    #[cfg(test)]
    pub(crate) tick_work: TickWork,
    pub(crate) garrison_orders: BTreeMap<EntityId, EntityId>,
    /// Derived from live Steward garrison orders, at most one per Town Center.
    pub(crate) garrison_stewards: BTreeMap<EntityId, EntityId>,
    pub(crate) garrison_reservations: BTreeMap<EntityId, u8>,
    pub(crate) occupied_garrisons: BTreeSet<EntityId>,
    pub(crate) ejections: BTreeMap<EntityId, (TileRect, Vec<GarrisonedUnit>)>,
    pub(crate) cargo: BTreeMap<EntityId, Vec<EntityId>>,
    pub(crate) transport_journeys: BTreeMap<EntityId, super::super::navy::Journey>,
    pub(crate) embarked: BTreeMap<EntityId, EntityId>,
    pub(crate) players: BTreeMap<PlayerId, PlayerEconomy>,
    pub(crate) income: BTreeMap<PlayerId, income::Income>,
    pub(crate) buildings: BTreeMap<EntityId, GameBuilding>,
    pub(crate) farm_production: BTreeMap<EntityId, farm::FarmProduction>,
    // Derived ordered worksets; building queues remain the canonical state.
    pub(crate) active_training: BTreeSet<EntityId>,
    pub(crate) active_research: BTreeSet<EntityId>,
    /// Every building per player and kind, ascending by id: drop-off
    /// candidates and requirements read one player's buildings
    /// without scanning the world. Maintained as buildings are placed and
    /// removed; completion is checked per query.
    pub(crate) dropoffs: BTreeMap<(PlayerId, BuildingKind), Vec<EntityId>>,
    /// Derived complete road tiles; empty worlds skip all road lookups.
    pub(crate) roads: BTreeSet<TileCoord>,
    pub(crate) blocked_tiles: BTreeSet<TileCoord>,
    pub(crate) open_gates: BTreeSet<EntityId>,
    /// Every gate building, ascending by id, maintained as buildings are
    /// placed and removed: the gate tick visits gates, never every building.
    pub(crate) gates: BTreeSet<EntityId>,
    pub(crate) scouts: BTreeMap<EntityId, scouting::AutoScout>,
    pub(crate) scout_searches: BTreeSet<EntityId>,
    pub(crate) scout_cursor: Option<EntityId>,
    pub(crate) gatherers: gathering::Gatherers,
    pub(crate) work_positions: gathering::WorkPositions,
    /// Tick-local search allowance, reset before movement; not persistent state.
    pub(crate) gather_repick_budget: u8,
    /// Replay oracle switch: every gatherer steps every tick (sleep off).
    pub(crate) no_sleep: bool,
    /// Units changed after last tick's gathering loop: they wake dozers.
    pub(crate) late_changed: Vec<EntityId>,
    /// Benchmark counters (`GameWorld::gather_work`). Not replay state.
    pub(crate) gather_work: super::gathering::GatherCounters,
    pub(crate) builders: BTreeMap<EntityId, BuildTask>,
    pub(crate) depletions: Vec<aoe_map::Depletion>,
    pub(crate) limits: WorldLimits,
}
