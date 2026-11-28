//! Mutable economy indexes and test-only work accounting.
use super::{
    BuildingKind, GameBuilding, GarrisonedUnit, PlayerEconomy, WorldLimits, farm, gathering,
    scouting, steward,
};
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};
use std::collections::{BTreeMap, BTreeSet};
mod blocked;
mod census;
mod income;
mod query;
use blocked::BlockedTiles;

#[cfg(test)]
mod blocked_tests;

#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct TickWork {
    pub(crate) training_buildings: usize,
    pub(crate) research_buildings: usize,
    pub(crate) spawn_units: usize,
    /// Cumulative gatherer visits, and those that slept through the step.
    pub(crate) gather_visits: usize,
    pub(crate) gather_slept: usize,
    pub(crate) builder_slept: usize,
    /// Occupant entries actually visited by garrison healing.
    pub(crate) garrison_units: usize,
    /// Cumulative buildings read by counts of a player's population space, and
    /// training buildings looked at when a player's population changed.
    pub(crate) housing_reads: usize,
    pub(crate) training_reads: usize,
}

#[derive(Debug, Default)]
pub(crate) struct Economy {
    /// Physical tile/edge answers shared across movers, separately for land
    /// and water. Collision changes invalidate only affected chunks.
    pub(crate) movement_ground: [std::sync::Mutex<aoe_map::RouteGroundCache>; 2],
    /// The land steps a leg search may take from each tile, shared by every
    /// player's search away from gates; forgotten with `movement_ground`.
    pub(crate) step_masks: aoe_map::RouteStepMasks,
    pub(crate) land: std::sync::Mutex<super::land::Cache>,
    pub(crate) walking: super::land::walking::ZoneCache,
    pub(crate) census: std::sync::Mutex<census::Cache>,
    /// Living villagers on the map, used by foundation placement.
    pub(crate) villagers: BTreeMap<PlayerId, BTreeSet<EntityId>>,
    pub(crate) vassals: super::vassal::Manager,
    pub(crate) generals: super::general::Manager,
    pub(crate) stewards: steward::Manager,
    pub(crate) steward_homes: BTreeMap<EntityId, steward::Home>,
    #[cfg(test)]
    pub(crate) tick_work: TickWork,
    pub(crate) garrison_orders: BTreeMap<EntityId, EntityId>,
    pub(crate) shelter_intents: BTreeMap<EntityId, super::garrison::controls::ShelterIntent>,
    pub(crate) town_bells: BTreeSet<EntityId>,
    /// Derived from live Steward garrison orders, at most one per Town Center.
    pub(crate) garrison_stewards: BTreeMap<EntityId, EntityId>,
    pub(crate) garrison_reservations: BTreeMap<EntityId, u8>,
    pub(crate) occupied_garrisons: BTreeSet<EntityId>,
    pub(crate) garrison_timers: super::garrison::timers::Timers,
    pub(crate) ejections: BTreeMap<EntityId, (TileRect, Vec<GarrisonedUnit>)>,
    pub(crate) cargo: crate::game_world::changes::Reports<Vec<EntityId>>,
    pub(crate) transport_journeys: BTreeMap<EntityId, super::super::navy::Journey>,
    pub(crate) embarked: BTreeMap<EntityId, EntityId>,
    pub(crate) players: BTreeMap<PlayerId, PlayerEconomy>,
    /// Derived from living units, including garrisons and passengers.
    pub(crate) upkeep_units: BTreeMap<PlayerId, u32>,
    /// Owners whose population space is counted when the bulk load under way
    /// ends (`GameWorld::count_housing_once`); `None` outside one.
    pub(crate) housing_due: Option<BTreeSet<PlayerId>>,
    pub(crate) market_recovery: super::market::MarketRecovery,
    pub(crate) income: BTreeMap<PlayerId, income::Income>,
    pub(crate) buildings: BTreeMap<EntityId, GameBuilding>,
    pub(crate) farm_production: BTreeMap<EntityId, farm::FarmProduction>,
    // Derived ordered worksets; building queues remain the canonical state.
    pub(crate) queues: super::training::timers::QueueTimers,
    pub(crate) active_training: BTreeSet<EntityId>,
    pub(crate) active_research: BTreeSet<EntityId>,
    /// Every building per player and kind, ascending by id: drop-off
    /// candidates and requirements read one player's buildings
    /// without scanning the world. Maintained as buildings are placed and
    /// removed; completion is checked per query.
    pub(crate) dropoffs: BTreeMap<(PlayerId, BuildingKind), Vec<EntityId>>,
    /// Derived complete road tiles; empty worlds skip all road lookups.
    pub(crate) roads: BTreeSet<TileCoord>,
    pub(crate) blocked_tiles: BlockedTiles,
    pub(crate) open_gates: BTreeSet<EntityId>,
    /// Every gate building, ascending by id, maintained as buildings are
    /// placed and removed: the gate tick visits gates, never every building.
    pub(crate) gates: BTreeSet<EntityId>,
    /// Footprint tiles of every gate in `gates`, complete or not: region
    /// ground reads the gates near a chunk instead of rebuilding them all.
    pub(crate) gate_tiles: BTreeMap<TileCoord, EntityId>,
    /// Automatic Trade Cart and Cog runs; not saved (see `trade`).
    pub(crate) trade: BTreeMap<EntityId, super::trade::TradeRun>,
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
    pub(crate) dispatch: super::construction::dispatch::ConstructionDispatch,
    pub(crate) foundations: super::construction::Foundations,
    pub(crate) builders: super::construction::Builders,
    pub(crate) construction: super::construction::ConstructionTimers,
    pub(crate) resource_recovery: super::super::resources::regeneration::ResourceRecovery,
    pub(crate) depletions: Vec<aoe_map::Depletion>,
    pub(crate) limits: WorldLimits,
}

/// Test-only work accounting, kept here: the architecture check reads a file
/// only up to its first test-only item.
impl Economy {
    #[cfg_attr(not(test), allow(unused_variables))]
    pub(super) fn note_housing_read(&mut self, player: PlayerId) {
        #[cfg(test)]
        {
            self.tick_work.housing_reads += super::buildings::BUILDING_RULES
                .iter()
                .filter(|rules| rules.population_support > 0)
                .filter_map(|rules| self.dropoffs.get(&(player, rules.kind)))
                .map(Vec::len)
                .sum::<usize>();
        }
    }

    #[cfg_attr(not(test), allow(unused_variables))]
    pub(super) fn note_training_reads(&mut self, queues: usize) {
        #[cfg(test)]
        {
            self.tick_work.training_reads += queues;
        }
    }
}
