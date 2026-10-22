//! Deterministic parallel movement. The proposal phase computes, for every
//! mover whose tick needs no shared planning state, the unit's next state from
//! an immutable snapshot of the world (positions, orders, terrain cache,
//! buildings, research, vision). It writes nothing shared, so the result does
//! not depend on how the movers are split over threads. The apply phase then
//! runs in the planning-cursor rotation order of the sequential engine and
//! performs every shared write (unit store, vision, spatial buckets, output
//! lists). A proposal that assumed the route planner capacity was exhausted
//! re-checks that at apply time. Movers that search, fail, or cross two
//! waypoints in one tick are not proposed (or are rejected at apply); they run
//! the sequential path in the same order, so they consume the shared planning
//! budget exactly as before.
use super::parallel::par_map;
use super::{facing_for, interpolate};
use crate::PlayerEconomy;
use crate::game_path::{next_waypoint, segment_length};
use crate::{GameUnit, GameWorld, MovementOrder, UnitTask};
use aoe_core::{ChunkCoord, EntityId, PlayerId, WorldPosition};
use aoe_map::RoutePlanner;

/// A proposed tick of one mover.
pub(crate) struct FastStep {
    state: GameUnit,
    order: Option<MovementOrder>,
    /// Route tiles consumed from the front of the unit's route.
    pops: usize,
    keeps_moving: bool,
    bucket: ChunkCoord,
    vision_stale: bool,
    /// The unit would have searched but for exhausted planner capacity.
    assumes_capacity_exhausted: bool,
}

/// Per-tick dense lookups that replace ordered-map reads on the per-mover path.
struct Tables {
    /// Owner economy by player id; players past the table use the map.
    economies: Vec<Option<PlayerEconomy>>,
    /// Entities with a gather, hunt, attack or build job: their task is not
    /// overwritten by movement.
    busy: Vec<bool>,
}

impl Tables {
    fn build(world: &GameWorld) -> Self {
        let mut economies = Vec::new();
        for economy in world.players().filter(|p| usize::from(p.player.0) < 4_096) {
            let at = usize::from(economy.player.0);
            if at >= economies.len() {
                economies.resize(at + 1, None);
            }
            economies[at] = Some(economy);
        }
        let mut busy = vec![false; world.next_entity_id as usize];
        let jobs = world
            .economy
            .gatherers
            .keys()
            .chain(world.wildlife.hunter_ids())
            .chain(world.combat.attackers.keys())
            .chain(world.economy.builders.keys());
        for id in jobs {
            if let Some(slot) = busy.get_mut(id.0 as usize) {
                *slot = true;
            }
        }
        Self { economies, busy }
    }

    fn economy(&self, world: &GameWorld, player: PlayerId) -> Option<PlayerEconomy> {
        match self.economies.get(usize::from(player.0)) {
            Some(found) => *found,
            None => world.player_economy(player),
        }
    }

    fn busy(&self, id: EntityId) -> bool {
        self.busy.get(id.0 as usize).copied().unwrap_or(true)
    }
}

enum Next {
    Segment(WorldPosition),
    Waiting,
}

impl GameWorld {
    /// Proposals in rotation order: entry `turn` belongs to
    /// `movers[(start + turn) % movers.len()]`.
    pub(crate) fn propose_steps(&self, movers: &[EntityId], start: usize) -> Vec<Option<FastStep>> {
        let count = movers.len();
        let tables = Tables::build(self);
        par_map(count, |turn| {
            self.propose_step(movers[(start + turn) % count], &tables)
        })
    }

    /// The single-position-update cases of the sequential waypoint loop. `None`
    /// hands the unit to the sequential path.
    fn propose_step(&self, id: EntityId, tables: &Tables) -> Option<FastStep> {
        let index = *self.lookup.get(&id)?;
        let stored = &self.units[index];
        let mut order = stored.order?;
        if stored
            .planner
            .as_ref()
            .is_some_and(RoutePlanner::wants_lookahead)
        {
            return None;
        }
        let mut state = stored.state;
        let economy = tables.economy(self, state.player);
        let (speed, denominator) = self.movement_speed_with(state.kind, economy);
        let mut speed_budget = u128::from(order.speed_carry) + u128::from(speed);
        order.speed_carry = 0;
        let remaining = order.segment_length.saturating_sub(order.travelled);
        let step = u128::from(remaining).min(speed_budget / u128::from(denominator)) as u32;
        order.travelled += step;
        speed_budget -= u128::from(step) * u128::from(denominator);
        let arrived = order.travelled >= order.segment_length;
        let position = if arrived {
            order.waypoint
        } else {
            interpolate(order, order.travelled)
        };
        let (previous_tile, tile) = (state.position.tile_floor(), position.tile_floor());
        if !matches!(
            self.unit_step_clear(state.kind, previous_tile, tile),
            Ok(true)
        ) || !self.building_step_clear(previous_tile, tile)
        {
            return None;
        }
        state.previous_position = state.position;
        state.position = position;
        let mut pops = 0;
        let mut keeps_moving = true;
        let mut assumes_capacity_exhausted = false;
        let final_order;
        if !arrived {
            order.speed_carry = speed_budget as u64;
            state.moving = true;
            state.planning = false;
            final_order = Some(order);
        } else if order.waypoint == order.destination && stored.route.is_empty() {
            if stored.planner.is_some() {
                return None;
            }
            state.moving = false;
            state.planning = false;
            final_order = None;
            keeps_moving = false;
        } else {
            let next = match stored
                .route
                .front()
                .inspect(|_| pops = 1)
                .and_then(|tile| WorldPosition::from_tile_center(*tile).ok())
                .or_else(|| {
                    (order.waypoint != order.destination
                        && order.waypoint.tile_floor() == order.destination.tile_floor())
                    .then_some(order.destination)
                }) {
                Some(next) => Next::Segment(next),
                None => self.propose_map_segment(stored, order, &mut assumes_capacity_exhausted)?,
            };
            match next {
                Next::Waiting => {
                    order.speed_carry = (speed_budget % u128::from(denominator)) as u64;
                    state.moving = false;
                    state.planning = true;
                    final_order = Some(order);
                }
                Next::Segment(next) => {
                    let dx = i64::from(next.x) - i64::from(order.waypoint.x);
                    let dy = i64::from(next.y) - i64::from(order.waypoint.y);
                    let mut moved = MovementOrder {
                        origin: order.waypoint,
                        waypoint: next,
                        segment_length: segment_length(dx, dy),
                        travelled: 0,
                        speed_carry: 0,
                        ..order
                    };
                    state.facing = facing_for(dx, dy, state.facing);
                    if speed_budget >= u128::from(denominator) {
                        // A second waypoint in one tick moves the unit twice.
                        return None;
                    }
                    moved.speed_carry = speed_budget as u64;
                    state.moving = true;
                    state.planning = false;
                    final_order = Some(moved);
                }
            }
        }
        if !tables.busy(id) {
            state.task = if state.moving || state.planning {
                UnitTask::Moving
            } else if state.carried.is_some() {
                UnitTask::Carrying
            } else {
                UnitTask::Idle
            };
        }
        Some(FastStep {
            vision_stale: self.unit_vision_stale(&state, economy),
            bucket: ChunkCoord::from_position(position),
            state,
            order: final_order,
            pops,
            keeps_moving,
            assumes_capacity_exhausted,
        })
    }

    /// `next_map_segment` for the cases that touch no shared state.
    fn propose_map_segment(
        &self,
        stored: &crate::game_world::StoredUnit,
        order: MovementOrder,
        assumes_capacity_exhausted: &mut bool,
    ) -> Option<Next> {
        if !self.has_route_navigation() {
            return (order.waypoint != order.destination)
                .then(|| next_waypoint(order.waypoint, order.target_tile, order.destination))
                .map(Next::Segment);
        }
        let needs_search = stored
            .planner
            .as_ref()
            .is_none_or(|planner| !planner.has_route_continuation());
        if needs_search && self.planner_capacity_exhausted(self.lookup[&stored.state.id]) {
            *assumes_capacity_exhausted = true;
            return Some(Next::Waiting);
        }
        None
    }

    /// Commits a proposal: the shared writes of the sequential engine, in the
    /// same order (position, wildlife cells, vision, bucket). Returns `false`, writing nothing,
    /// when an assumption of the proposal no longer holds.
    pub(crate) fn apply_step(
        &mut self,
        step: FastStep,
        still_moving: &mut Vec<EntityId>,
        changed: &mut Vec<EntityId>,
    ) -> bool {
        let id = step.state.id;
        let Some(&index) = self.lookup.get(&id) else {
            return false;
        };
        if step.assumes_capacity_exhausted && !self.planner_capacity_exhausted(index) {
            return false;
        }
        let stored = &mut self.units[index];
        let previous_tile = stored.state.position.tile_floor();
        stored.state = step.state;
        stored.order = step.order;
        if step.pops > 0 {
            stored.route.pop_front();
        }
        // Wildlife's unit cells follow every step, before vision, as in the
        // sequential path.
        self.track_unit_step(id, previous_tile, &step.state);
        if step.vision_stale {
            self.update_unit_vision(id);
        }
        if self.units[index].bucket != step.bucket {
            self.move_bucket(index, step.bucket);
        }
        if step.keeps_moving {
            still_moving.push(id);
        }
        changed.push(step.state.id);
        true
    }
}
