use crate::game_movement::MapRoutePlan;
use crate::game_path::{next_waypoint, route_waypoint, segment_length};
use crate::movement_speed::cavalry_config;
use crate::{GameQueryStats, navigation_cache::NavigationCache, terrain::Terrain};
use aoe_core::{
    ChunkCoord, EntityId, FIXED_SUBUNITS_PER_TILE, PlayerId, SPATIAL_CHUNK_TILES,
    TILE_GROUND_RADIUS_SUBUNITS, Tick, TileCoord, TileRect, WorldConfig, WorldPosition, WorldRect,
};
use aoe_map::{Depletion, MovementOutcome, ResourceOverlayError, RoutePlanner};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

mod combat;
mod economy;
mod facing;
mod hash;
mod resources;
mod spawning;
pub use combat::*;
pub use economy::{
    BUILDING_RULES, BuildingKind, BuildingRules, CarriedResource, ConstructionState, EconomyError,
    GameBuilding, MAX_BUILD_GROUP, MAX_POPULATION, MAX_WALL_SEGMENTS, PlayerEconomy, Stockpile,
    TrainingEntry, UnitKind, UnitTask, VILLAGE_RULES, VillageRules, trainable_units,
};
pub use resources::PreparedResourceDepletion;
pub use facing::Facing;
pub(crate) use facing::facing_for;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    pub hp: u16,
    pub max_hp: u16,
    pub attack_target: Option<EntityId>,
    pub last_hit: Option<HitRecord>,
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
    pub(crate) state: GameUnit,
    pub(crate) order: Option<MovementOrder>,
    pub(crate) bucket: ChunkCoord,
    pub(crate) bucket_slot: usize,
    pub(crate) route: VecDeque<TileCoord>,
    pub(crate) planner: Option<RoutePlanner>,
    pub(crate) last_movement_error: Option<GameWorldError>,
    pub(crate) next_hit_tick: u64, // no hit before this tick
}
#[derive(Debug)]
pub struct GameWorld {
    pub(crate) config: WorldConfig,
    pub(crate) next_entity_id: u64,
    pub(crate) economy: economy::Economy,
    pub(crate) combat: combat::Combat,
    pub(crate) tick: Tick,
    pub(crate) units: Vec<StoredUnit>,
    pub(crate) lookup: BTreeMap<EntityId, usize>,
    pub(crate) chunks: BTreeMap<ChunkCoord, Vec<EntityId>>,
    pub(crate) active_movers: Vec<EntityId>,
    pub(crate) terrain: Terrain,
    pub(crate) planning_budget: u32,
    pub(crate) navigation_cache: NavigationCache,
    pub(crate) planning_cursor: usize,
    pub(crate) active_planner_count: usize,
    pub(crate) active_route_searches: usize,
}
impl GameWorld {
    pub fn new(config: WorldConfig) -> Result<Self, GameWorldError> {
        config.validate()?;
        Ok(Self::from_valid_config(config))
    }

    fn from_valid_config(config: WorldConfig) -> Self {
        Self {
            terrain: Terrain::uniform(config.seed.0),
            config,
            tick: Tick(0),
            next_entity_id: 0,
            economy: economy::Economy::default(),
            combat: Default::default(),
            units: Vec::new(),
            lookup: BTreeMap::new(),
            chunks: BTreeMap::new(),
            active_movers: Vec::new(),
            planning_budget: crate::MAX_ROUTE_WORK_PER_TICK,
            navigation_cache: NavigationCache::default(),
            planning_cursor: 0,
            active_planner_count: 0,
            active_route_searches: 0,
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
                    radius + (next_random(&mut random) % hotspot_span as u64) as i64,
                    radius + (next_random(&mut random) % hotspot_span as u64) as i64,
                )
            } else {
                (
                    radius + (next_random(&mut random) % span_x as u64) as i64,
                    radius + (next_random(&mut random) % span_y as u64) as i64,
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
        let depletion = self.terrain.deplete_resource(id, requested)?;
        if depletion.became_nonblocking {
            self.resource_collision_changed();
        }
        Ok(depletion)
    }

    pub fn issue_move(
        &mut self,
        id: EntityId,
        destination: WorldPosition,
    ) -> Result<bool, GameWorldError> {
        let result = self.issue_task_move(id, destination)?;
        self.economy.gatherers.remove(&id);
        self.cancel_attack(id);
        self.economy.builders.remove(&id);
        if let Some(&index) = self.lookup.get(&id) {
            self.units[index].state.task = if result {
                UnitTask::Moving
            } else if self.units[index].state.carried.is_some() {
                UnitTask::Carrying
            } else {
                UnitTask::Idle
            };
        }
        Ok(result)
    }

    pub(crate) fn issue_task_move(
        &mut self,
        id: EntityId,
        destination: WorldPosition,
    ) -> Result<bool, GameWorldError> {
        let index = *self.lookup.get(&id).ok_or(GameWorldError::UnknownEntity)?;
        if self.units[index].state.hp == 0 {
            return Err(GameWorldError::UnknownEntity);
        }
        let destination = self.config.snap_ground_position(destination);
        let passable = self
            .terrain
            .passable_with_cancel(destination.tile_floor(), self.config, &|| false)
            .map_err(GameWorldError::Environment)?;
        if !passable
            || self
                .economy
                .blocked_tiles
                .contains(&destination.tile_floor())
        {
            return Err(GameWorldError::InvalidPosition);
        }
        let origin = self.units[index].state.position;
        let speed_carry = self.units[index].order.map_or(0, |order| order.speed_carry);
        self.clear_planner(index);
        self.units[index].last_movement_error = None;
        if origin == destination {
            self.units[index].order = None;
            self.units[index].route.clear();
            self.units[index].state.moving = false;
            self.units[index].state.planning = false;
            return Ok(false);
        }
        let target_tile = destination.tile_floor();
        let (waypoint, route, planner, planning) =
            match self.next_map_route(origin.tile_floor(), target_tile) {
                Some(MapRoutePlan::Outcome(MovementOutcome::Path(path))) => {
                    let (waypoint, route) = route_waypoint(path.tiles, origin, destination)?;
                    (waypoint, route, None, false)
                }
                Some(MapRoutePlan::Segment(path, planner)) => {
                    let (waypoint, route) = route_waypoint(path.tiles, origin, destination)?;
                    (waypoint, route, planner, false)
                }
                Some(MapRoutePlan::Outcome(MovementOutcome::InvalidDestination)) => {
                    return Err(GameWorldError::InvalidPosition);
                }
                Some(MapRoutePlan::Outcome(MovementOutcome::Unreachable)) => {
                    return Err(GameWorldError::Unreachable);
                }
                Some(MapRoutePlan::Outcome(MovementOutcome::BudgetExceeded))
                | Some(MapRoutePlan::SearchLimit) => {
                    return Err(GameWorldError::PathBudgetExceeded);
                }
                Some(MapRoutePlan::Pending(planner)) => (origin, VecDeque::new(), planner, true),
                Some(MapRoutePlan::Environment(error)) => {
                    return Err(GameWorldError::Environment(error));
                }
                None => (
                    next_waypoint(origin, target_tile, destination),
                    VecDeque::new(),
                    None,
                    false,
                ),
            };
        let dx = i64::from(waypoint.x) - i64::from(origin.x);
        let dy = i64::from(waypoint.y) - i64::from(origin.y);
        let length = segment_length(dx, dy);
        self.units[index].state.previous_position = origin;
        self.units[index].state.facing = facing_for(dx, dy, self.units[index].state.facing);
        self.units[index].state.moving = !planning;
        self.units[index].state.planning = planning;
        self.units[index].route = route;
        self.store_planner(index, planner);
        self.units[index].order = Some(MovementOrder {
            origin,
            destination,
            waypoint,
            target_tile,
            segment_length: length,
            travelled: 0,
            speed_carry,
        });
        if self.active_movers.binary_search(&id).is_err() {
            let insert_at = self
                .active_movers
                .binary_search(&id)
                .unwrap_or_else(|index| index);
            self.active_movers.insert(insert_at, id);
        }
        Ok(true)
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
            .filter(|unit| world_rect.contains(unit.position))
            .collect::<Vec<_>>();
        stats.returned_units = result.len() as u32;
        (result, stats)
    }

    pub(crate) fn move_bucket(&mut self, index: usize, bucket: ChunkCoord) {
        let old = self.units[index].bucket;
        let slot = self.units[index].bucket_slot;
        let Some(ids) = self.chunks.get_mut(&old) else {
            return;
        };
        // `swap_remove` puts the bucket's last unit into the vacated slot.
        ids.swap_remove(slot);
        let displaced = ids.get(slot).copied();
        let emptied = ids.is_empty();
        if let Some(displaced_index) = displaced.and_then(|id| self.lookup.get(&id).copied()) {
            self.units[displaced_index].bucket_slot = slot;
        }
        if emptied {
            self.chunks.remove(&old);
        }
        let new_slot = self.chunks.entry(bucket).or_default().len();
        self.chunks
            .entry(bucket)
            .or_default()
            .push(self.units[index].state.id);
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

fn next_random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

#[cfg(test)]
mod tests;
