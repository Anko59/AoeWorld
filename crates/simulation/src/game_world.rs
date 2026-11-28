use crate::movement_speed::cavalry_config;
use crate::{GameQueryStats, navigation_cache::NavigationCache, terrain::Terrain};
use aoe_core::{
    ChunkCoord, EntityId, FIXED_SUBUNITS_PER_TILE, PlayerId, SPATIAL_CHUNK_TILES,
    TILE_GROUND_RADIUS_SUBUNITS, Tick, TileCoord, TileRect, WorldConfig, WorldPosition, WorldRect,
};
use aoe_map::{Depletion, ResourceNode, ResourceOverlayError, RoutePlanner};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex};
#[cfg(test)]
#[path = "game_world/tests/scale_hooks/unit_access.rs"]
mod unit_access;
/// One chunk's shared candidate resource nodes.
pub(crate) type ChunkNodes = Arc<Vec<ResourceNode>>;
#[path = "game_world/changes/mod.rs"]
mod changes;
#[path = "terrain/clearance.rs"]
mod clearance;
mod combat;
#[path = "game_world/diplomacy/mod.rs"]
mod diplomacy;
mod economy;
mod facing;
#[path = "game_world/formation/mod.rs"]
mod formation;
mod hash;
#[path = "game_world/orders/queue.rs"]
mod move_queue;
pub use move_queue::MovementWaypoint;
use move_queue::PlayerMoveQueue;
mod movement_step;
#[path = "game_world/navy/mod.rs"]
mod navy;
mod orders;
pub use orders::{ISSUER_SPANS, OrderDemand, OrderIssuer, OrderRedundancy};
pub mod parallel;
#[path = "game_world/relics/mod.rs"]
pub(crate) mod relics;
mod research;
mod resources;
#[path = "game_world/snapshot/mod.rs"]
pub(crate) mod snapshot;
pub use snapshot::{RestoreReport, SnapshotError, WORLD_SNAPSHOT_VERSION, WorldSnapshot};
mod route_scheduler;
pub(crate) use route_scheduler::Pass;
mod spacing;
mod spawning;
mod unit_index;
pub(crate) use unit_index::{ChunkBuckets, IdSet, UnitIndex};
#[path = "game_world/visibility/mod.rs"]
mod visibility;
mod wildlife;
pub use combat::*;
pub use economy::{
    BUILDING_RULES, BUILDINGS_PER_PLAYER, BehaviourScene, BuildingKind, BuildingRules,
    CarriedResource, ConstructionState, ConstructionWork, EconomyError, GameBuilding,
    GarrisonedUnit, GatherWork, GeneralState, GeneralStatus, GeneralWork, HoldingSpan, LandControl,
    LateGameSeat, LateGameSeeded, MARKET_RULES, MAX_BUILD_GROUP, MarketPrices, PlayerEconomy,
    RallyPoint, RallyTarget, STEWARD_RULES, STEWARD_SPANS, STEWARD_STALL_SECONDS, SquadKind,
    SquadStatus, StewardRules, StewardSettings, StewardStatus, StewardWait, StewardWork, Stockpile,
    TickPhase, TrainingEntry, UnitKind, UnitTask, VASSAL_LIMIT, VASSAL_RADIUS, VILLAGE_RULES,
    VassalFocus, VassalJob, VassalJobs, VassalSquad, VassalStatus, VassalWait, VillageRules,
    WorldLimits, trainable_units,
};
pub use facing::Facing;
pub(crate) use facing::facing_for;
pub use formation::GroupProbe;
pub use research::*;
pub use resources::PreparedResourceDepletion;
pub use route_scheduler::{
    PASS_NAMES, PLANNING_SPANS, PassCount, PlanningProbe, ROUTE_CATEGORIES, ROUTE_DISTANCES,
    ROUTE_FAILURES, ROUTE_REACH, ROUTE_REQUESTERS, RouteAttribution, RouteCaller, RouteClass,
    RouteQueueBreakdown, RouteQueueCell, RouteSchedulerStats,
};
pub use visibility::{HumanView, StateCensus, TileVisibility};
pub use wildlife::*;
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GameUnit {
    pub id: EntityId,
    pub player: PlayerId,
    pub position: WorldPosition,
    pub previous_position: WorldPosition,
    pub moving: bool,
    pub planning: bool,
    pub facing: Facing,
    pub kind: UnitKind,
    pub task: UnitTask,
    pub carried: Option<CarriedResource>,
    /// Derived from the sparse relic carrier index on restore.
    #[serde(skip)]
    pub holds_relic: bool,
    pub hp: u16,
    pub max_hp: u16,
    pub attack_target: Option<EntityId>,
    pub last_hit: Option<HitRecord>,
    pub death_tick: Option<Tick>,
    pub death_hunt: Option<AnimalKind>,
    /// Authoritative cavalry exploration toggle.
    pub auto_scout: bool,
    /// Promoted squad officer; retained on the corpse for its native hero poses.
    pub sergeant: bool,
    pub stance: aoe_core::CombatStance,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MovementOrder {
    pub origin: WorldPosition,
    pub destination: WorldPosition,
    pub waypoint: WorldPosition,
    pub target_tile: TileCoord,
    pub segment_length: u32,
    pub travelled: u32,
    /// Fractional subunits already earned for the next movement step.
    pub speed_carry: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum GameWorldError {
    #[error("world configuration is invalid: {0}")]
    InvalidConfig(#[from] aoe_core::CoordinateError),
    #[error("position is outside the valid map ground")]
    InvalidPosition,
    #[error("destination is unreachable from the unit position")]
    Unreachable,
    #[error("path planning exceeded its deterministic work budget")]
    PathBudgetExceeded,
    #[error("entity does not exist")]
    UnknownEntity,
    #[error(
        "starting-location search limit reached; absence of a suitable start is not established"
    )]
    StartSearchLimit,
    #[error("entity id space is exhausted")]
    EntityIdExhausted,
    #[error("prepared map terrain is invalid")]
    InvalidTerrain,
    #[error("prepared map environment lookup failed: {0}")]
    Environment(#[from] aoe_map::EnvironmentPageError),
}
#[derive(Debug)]
pub(crate) struct StoredUnit {
    pub(crate) formation: Option<EntityId>,
    pub(crate) state: GameUnit,
    pub(crate) order: Option<MovementOrder>,
    pub(crate) obstruction: Option<movement_step::Obstruction>,
    pub(crate) bucket: ChunkCoord,
    pub(crate) bucket_slot: usize,
    pub(crate) wildlife_slot: usize,
    pub(crate) route: VecDeque<WorldPosition>,
    pub(crate) planner: Option<Box<RoutePlanner>>,
    pub(crate) last_movement_error: Option<GameWorldError>,
    pub(crate) next_hit_tick: u64, // no hit before this tick
    /// Tick it last started waiting for a route (the fair pass serves first).
    pub(crate) planning_since: u64,
    /// The route it waits for had its whole fast attempt (see
    /// `route_scheduler::issue`); it now needs a search slot.
    pub(crate) fast_route_tried: bool,
    /// Last order refused as unreachable: from, to, tick (`route_scheduler::order`).
    pub(crate) unreachable_memo: Option<(TileCoord, TileCoord, u64)>,
    /// Diagnostics only: the call site that ordered the current route.
    pub(crate) route_caller: Option<&'static std::panic::Location<'static>>,
    pub(crate) route_issuer: Option<OrderIssuer>,
    /// Times this order's far group wait was renewed (`deadline`).
    pub(crate) route_renewals: u8,
}
#[derive(Debug)]
pub struct GameWorld {
    pub(crate) config: WorldConfig,
    pub(crate) next_entity_id: u64,
    pub(crate) economy: economy::Economy,
    pub(crate) combat: combat::Combat,
    pub(crate) diplomacy: diplomacy::Diplomacy,
    pub(crate) wildlife: wildlife::Wildlife,
    pub(crate) relics: relics::Relics,
    pub(crate) vision: visibility::Vision,
    pub(crate) tick: Tick,
    pub(crate) units: changes::Units,
    pub(crate) unit_changes: Vec<EntityId>,
    pub(crate) lookup: UnitIndex,
    /// The all-unit presence index: unit ids per 32-tile chunk.
    pub(crate) chunks: ChunkBuckets,
    /// Units with an order to walk, in id order: the Gauss-Seidel apply order.
    pub(crate) active_movers: IdSet,
    pub(crate) movement_scratch: movement_step::Scratch,
    pub(crate) yields: movement_step::Yields,
    pub(crate) rides: formation::Rides, // off-screen rides (`steering/ride.rs`)
    pub(crate) route_guides: route_scheduler::RouteGuides, // `route_scheduler/fields.rs`
    pub(crate) collisions: movement_step::CollisionIndex,
    /// Derived clearance valid only while one sequential mover owns the turn.
    pub(crate) work_approach_clearance: Option<(EntityId, WorldPosition, bool)>,
    pub(crate) melee_traffic_clearance: Option<(EntityId, WorldPosition, bool)>,
    pub(crate) local_refinement_left: u32,
    pub(crate) formations: BTreeMap<EntityId, formation::Group>,
    pub(crate) move_queues: BTreeMap<EntityId, PlayerMoveQueue>,
    pub(crate) waypoint_order: u32,
    pub(crate) destinations: spacing::Destinations,
    pub(crate) terrain: Terrain,
    pub(crate) clearance: Mutex<clearance::Field>,
    /// Candidate resource nodes per generator chunk (`chunk_resource_nodes`).
    pub(crate) resource_chunks: Mutex<crate::game_cache::ResourceCache>,
    /// No route budget in play (#978): each search takes its own slice. A
    /// test may hold the next fair pass to this much work.
    pub(crate) planning_budget: Option<u32>,
    pub(crate) navigation_preparation_budget: Mutex<u32>,
    /// As `planning_budget`, for this tick's fast attempts (`Some(0)` skips).
    pub(crate) fast_route_budget: Option<u32>,
    pub(crate) navigation_cache: NavigationCache,
    /// Land regions over the planner's ground: a cache, never hashed.
    pub(crate) regions: Mutex<[Option<aoe_map::Regions>; 3]>,
    pub(crate) planning_cursor: usize,
    pub(crate) active_planner_count: usize,
    pub(crate) active_route_searches: usize,
    pub(crate) partial_kits: u64,
    /// The order being routed needs its exact contact point (a task's work
    /// or build spot), not an area: reaching a nearby tile instead is no use.
    pub(crate) routing_contact: bool,
    pub(crate) route_slots: BTreeMap<PlayerId, route_scheduler::RouteSlots>,
    pub(crate) group_work: std::sync::Arc<route_scheduler::GroupWork>,
    pub(crate) order_diagnostics: orders::diagnostics::OrderDiagnostics,
    pub(crate) route_metrics: route_scheduler::RouteMetrics,
    pub(crate) planning_probe: Option<std::sync::Arc<dyn PlanningProbe>>,
    pub(crate) idle_spacing: spacing::IdleSpacing,
    /// Whole-population pass counters: diagnostics, never hashed.
    pub(crate) passes: route_scheduler::Passes,
    /// Units a collision change can affect (`resources.rs`). Derived, never hashed.
    pub(crate) collision_subscribers: resources::CollisionSubscribers,
}
impl GameWorld {
    pub fn new(config: WorldConfig) -> Result<Self, GameWorldError> {
        config.validate()?;
        Ok(Self::from_valid_config(config))
    }

    fn from_valid_config(config: WorldConfig) -> Self {
        Self {
            terrain: Terrain::uniform(config.seed.0),
            clearance: Mutex::new(Default::default()),
            config,
            tick: Tick(0),
            next_entity_id: 0,
            economy: economy::Economy::default(),
            combat: Default::default(),
            diplomacy: Default::default(),
            wildlife: Default::default(),
            relics: Default::default(),
            vision: Default::default(),
            units: Default::default(),
            unit_changes: Vec::new(),
            lookup: UnitIndex::new(),
            chunks: ChunkBuckets::default(),
            active_movers: IdSet::default(),
            movement_scratch: Default::default(),
            yields: Default::default(),
            rides: Default::default(),
            route_guides: Default::default(),
            work_approach_clearance: None,
            melee_traffic_clearance: None,
            collisions: Default::default(),
            local_refinement_left: 4,
            move_queues: BTreeMap::new(),
            waypoint_order: 0,
            formations: BTreeMap::new(),
            destinations: spacing::Destinations::default(),
            planning_budget: None,
            navigation_preparation_budget: Mutex::new(crate::MAX_NAV_PREPARATIONS_PER_TICK),
            fast_route_budget: None,
            navigation_cache: NavigationCache::default(),
            regions: Mutex::new(std::array::from_fn(|_| None)),
            resource_chunks: Mutex::new(Default::default()),
            planning_cursor: 0,
            active_planner_count: 0,
            active_route_searches: 0,
            partial_kits: 0,
            routing_contact: false,
            route_slots: BTreeMap::new(),
            group_work: Default::default(),
            order_diagnostics: Default::default(),
            route_metrics: Default::default(),
            planning_probe: None,
            idle_spacing: Default::default(),
            passes: Default::default(),
            collision_subscribers: Default::default(),
        }
    }

    pub fn with_cavalry(config: WorldConfig) -> Result<(Self, EntityId), GameWorldError> {
        let config = cavalry_config(config);
        let mut world = Self::new(config)?;
        let center = WorldPosition::new(
            config.width_tiles * FIXED_SUBUNITS_PER_TILE / 2,
            config.height_tiles * FIXED_SUBUNITS_PER_TILE / 2,
        );
        let id = world.spawn_unit(PlayerId(0), config.snap_ground_position(center))?;
        Ok((world, id))
    }

    pub fn with_population(
        config: WorldConfig,
        count: u32,
        hotspot_count: u32,
        players: u16,
    ) -> Result<Self, GameWorldError> {
        Self::with_population_in_extent(
            config,
            count,
            hotspot_count,
            players,
            config.width_tiles.min(config.height_tiles),
        )
    }

    pub fn with_population_in_extent(
        config: WorldConfig,
        count: u32,
        hotspot_count: u32,
        players: u16,
        extent_tiles: i32,
    ) -> Result<Self, GameWorldError> {
        let mut world = Self::new(config)?;
        world.set_fog_disabled(true);
        let radius = i64::from(TILE_GROUND_RADIUS_SUBUNITS);
        let width = i64::from(config.world_width_subunits()?);
        let height = i64::from(config.world_height_subunits()?);
        let extent = i64::from(
            extent_tiles
                .max(1)
                .min(config.width_tiles)
                .min(config.height_tiles),
        ) * i64::from(FIXED_SUBUNITS_PER_TILE);
        let span_x = (extent - radius).max(1);
        let span_y = (extent - radius).max(1);
        let hotspot_span = (128_i64 * i64::from(FIXED_SUBUNITS_PER_TILE) - 2 * radius).max(1);
        let mut random = config.seed.0.max(1);
        let player_count = u32::from(players.max(1));
        for raw in 0..count {
            let (x, y) = if raw < hotspot_count {
                (
                    radius + (spawning::next_random(&mut random) % hotspot_span as u64) as i64,
                    radius + (spawning::next_random(&mut random) % hotspot_span as u64) as i64,
                )
            } else {
                (
                    radius + (spawning::next_random(&mut random) % span_x as u64) as i64,
                    radius + (spawning::next_random(&mut random) % span_y as u64) as i64,
                )
            };
            let position =
                WorldPosition::from_i64(x.min(width - radius - 1), y.min(height - radius - 1))
                    .map_err(GameWorldError::InvalidConfig)?;
            world.spawn_unit(PlayerId((raw % player_count) as u16), position)?;
        }
        Ok(world)
    }

    pub fn default_with_cavalry(seed: aoe_core::Seed) -> (Self, EntityId) {
        let config = cavalry_config(WorldConfig {
            seed,
            ..WorldConfig::default()
        });
        let mut world = Self::from_valid_config(config);
        let center = WorldPosition::new(
            config.width_tiles * FIXED_SUBUNITS_PER_TILE / 2,
            config.height_tiles * FIXED_SUBUNITS_PER_TILE / 2,
        );
        let id = world
            .spawn_unit(PlayerId(0), config.snap_ground_position(center))
            .unwrap_or(EntityId(0));
        (world, id)
    }

    pub fn config(&self) -> WorldConfig {
        self.config
    }
    pub fn tick(&self) -> Tick {
        self.tick
    }
    pub fn terrain(&self) -> &Terrain {
        &self.terrain
    }
    pub fn unit_count(&self) -> usize {
        self.units.len()
    }

    pub fn unit_exists(&self, id: EntityId) -> bool {
        self.lookup.contains_key(&id)
    }

    pub fn occupied_chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub fn active_mover_count(&self) -> usize {
        self.active_movers.len()
    }

    pub fn units(&self) -> impl Iterator<Item = GameUnit> + '_ {
        self.units.iter().map(|unit| unit.state)
    }

    pub fn unit(&self, id: EntityId) -> Option<GameUnit> {
        self.lookup.get(&id).map(|index| self.units[*index].state)
    }

    pub fn movement_order(&self, id: EntityId) -> Option<MovementOrder> {
        self.lookup
            .get(&id)
            .and_then(|index| self.units[*index].order)
    }

    /// Returns the typed terminal reason for the most recent stopped movement,
    /// if the unit stopped because route execution failed.
    pub fn movement_failure(&self, id: EntityId) -> Option<GameWorldError> {
        self.lookup
            .get(&id)
            .and_then(|index| self.units[*index].last_movement_error)
    }

    /// Depletes an immutable-map resource; collision updates at exhaustion.
    pub fn deplete_resource(
        &mut self,
        id: u64,
        requested: u16,
    ) -> Result<Depletion, ResourceOverlayError> {
        Ok(self.prepare_resource_depletion(id, requested)?.commit())
    }

    pub fn query(&self, rect: TileRect) -> (Vec<GameUnit>, GameQueryStats) {
        let mut stats = GameQueryStats::default();
        if !rect.valid(self.config.width_tiles, self.config.height_tiles) {
            return (Vec::new(), stats);
        }
        let Ok(world_rect) = WorldRect::from_tiles(rect) else {
            return (Vec::new(), stats);
        };
        let first_x = rect.min.x.div_euclid(SPATIAL_CHUNK_TILES);
        let last_x = (rect.max.x - 1).div_euclid(SPATIAL_CHUNK_TILES);
        let first_y = rect.min.y.div_euclid(SPATIAL_CHUNK_TILES);
        let last_y = (rect.max.y - 1).div_euclid(SPATIAL_CHUNK_TILES);
        let mut candidates = BTreeSet::new();
        for y in first_y..=last_y {
            for x in first_x..=last_x {
                stats.visited_chunks += 1;
                if let Some(bucket) = self.chunks.get(&ChunkCoord { x, y }) {
                    stats.candidate_units += bucket.len() as u32;
                    candidates.extend(bucket.iter().copied());
                }
            }
        }
        let result = candidates
            .into_iter()
            .filter_map(|id| self.unit(id))
            .filter(|unit| !self.embarked(unit.id))
            .filter(|unit| world_rect.contains(unit.position))
            .collect::<Vec<_>>();
        stats.returned_units = result.len() as u32;
        (result, stats)
    }

    pub(crate) fn move_bucket(&mut self, index: usize, bucket: ChunkCoord) {
        let _span = self
            .planning_probe
            .as_ref()
            .and_then(|probe| probe.movement_span("index_chunks"));
        let old = self.units[index].bucket;
        self.combat.quiet.arrive(bucket, &self.units[index].state);
        let slot = self.units[index].bucket_slot;
        if self.chunks.get(&old).is_none() {
            return;
        }
        // `swap_remove` puts the bucket's last unit into the vacated slot.
        let owner = self.units[index].living_owner();
        let displaced = self.chunks.swap_remove(old, slot, owner);
        if let Some(displaced_index) = displaced.and_then(|id| self.lookup.get(&id).copied()) {
            self.units[displaced_index].bucket_slot = slot;
        }
        let new_slot = self.chunks.push(bucket, self.units[index].state.id, owner);
        self.units[index].bucket = bucket;
        self.units[index].bucket_slot = new_slot;
    }
}

pub(crate) fn interpolate(order: MovementOrder, travelled: u32) -> WorldPosition {
    let t = i128::from(travelled);
    let length = i128::from(order.segment_length);
    let x = i128::from(order.origin.x)
        + (i128::from(order.waypoint.x) - i128::from(order.origin.x)) * t / length;
    let y = i128::from(order.origin.y)
        + (i128::from(order.waypoint.y) - i128::from(order.origin.y)) * t / length;
    WorldPosition::new(x as i32, y as i32)
}

#[cfg(any(test, feature = "bench-fixtures"))]
#[path = "game_world/tests/real_terrain/fixtures/mod.rs"]
pub mod terrain_fixtures;
#[cfg(test)]
mod tests;
