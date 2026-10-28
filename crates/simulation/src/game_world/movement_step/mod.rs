//! Deterministic parallel movement. The proposal phase computes, for every
//! mover whose tick needs no shared planning state, the unit's next state from
//! an immutable snapshot of the world (positions, orders, terrain cache,
//! buildings, research, vision). It writes nothing shared, so the result does
//! not depend on how the movers are split over threads. The apply phase then
//! runs in the planning-cursor rotation order of the sequential engine and
//! performs every shared write (unit store, vision, spatial buckets, output
//! lists). A proposal re-checks follower spacing at apply time. Movers that
//! search, fail, or cross two waypoints in one tick are not proposed (or are
//! rejected at apply); they run
//! the sequential path in the same order, so they consume the shared planning
//! budget exactly as before.
use super::parallel::par_map_into;
use super::{facing_for, interpolate};
use crate::PlayerEconomy;
use crate::game_path::{next_waypoint, segment_length};
use crate::{GameUnit, GameWorld, MovementOrder, UnitTask};
use aoe_core::{ChunkCoord, EntityId, PlayerId, WorldPosition};

mod avoidance;
mod collision;
mod static_collision;
pub(crate) use collision::CollisionIndex;
mod lookahead;
mod refinement;
pub(crate) mod settling;
pub(crate) mod slots;
pub(crate) use settling::Obstruction;

/// A proposed tick of one mover.
#[derive(Debug)]
pub(crate) struct FastStep {
    index: usize,
    state: GameUnit,
    order: Option<MovementOrder>,
    /// Route tiles consumed from the front of the unit's route.
    pops: usize,
    keeps_moving: bool,
    bucket: ChunkCoord,
    vision_stale: bool,
    /// Earlier movers can change whether this unit must leave a gap.
    following: bool,
}

/// Per-tick dense lookups that replace ordered-map reads on the per-mover path.
#[derive(Debug, Default)]
struct Tables {
    /// Owner economy by player id; players past the table use the map.
    economies: Vec<Option<PlayerEconomy>>,
    /// Entities with a gather, hunt, attack or build job: their task is not
    /// overwritten by movement.
    busy: Vec<bool>,
}

impl Tables {
    fn refresh(&mut self, world: &GameWorld) {
        self.economies.clear();
        for economy in world.players().filter(|p| usize::from(p.player.0) < 4_096) {
            let at = usize::from(economy.player.0);
            if at >= self.economies.len() {
                self.economies.resize(at + 1, None);
            }
            self.economies[at] = Some(economy);
        }
        self.busy.clear();
        self.busy.resize(world.next_entity_id as usize, false);
        let jobs = world
            .economy
            .gatherers
            .keys()
            .chain(world.wildlife.hunter_ids())
            .chain(world.combat.attackers.keys())
            .chain(world.economy.builders.keys());
        for id in jobs {
            if let Some(slot) = self.busy.get_mut(id.0 as usize) {
                *slot = true;
            }
        }
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

/// Non-canonical high-water buffers shared by successive movement ticks.
#[derive(Debug, Default)]
pub(crate) struct Scratch {
    pub(crate) movers: Vec<EntityId>,
    pub(crate) proposals: Vec<Option<FastStep>>,
    tables: Tables,
    #[cfg(not(target_arch = "wasm32"))]
    jobs: Vec<usize>,
    #[cfg(not(target_arch = "wasm32"))]
    turns: Vec<usize>,
    #[cfg(not(target_arch = "wasm32"))]
    followers: super::spacing::Snapshot,
}

impl Scratch {
    pub(crate) fn take_step(&mut self, turn: usize) -> Option<FastStep> {
        #[cfg(not(target_arch = "wasm32"))]
        let slot = *self.turns.get(turn)?;
        #[cfg(target_arch = "wasm32")]
        let slot = turn;
        self.proposals.get_mut(slot)?.take()
    }
}

enum Next {
    Segment(WorldPosition),
    Waiting,
}

impl GameWorld {
    /// Proposals in rotation order: entry `turn` belongs to
    /// `movers[(start + turn) % movers.len()]`.
    pub(crate) fn propose_steps_into(
        &self,
        movers: &[EntityId],
        start: usize,
        scratch: &mut Scratch,
    ) {
        let count = movers.len();
        if count == 0 {
            scratch.proposals.clear();
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let Scratch {
                jobs,
                turns,
                followers,
                tables,
                proposals,
                ..
            } = scratch;
            jobs.clear();
            turns.clear();
            turns.resize(count, usize::MAX);

            // Only ordered movers enter the immutable proposal batch.
            for turn in 0..count {
                let id = movers[(start + turn) % count];
                if let Some(&index) = self.lookup.get(&id)
                    && self.units[index].order.is_some()
                {
                    turns[turn] = jobs.len();
                    jobs.push(index);
                }
            }
            if jobs.is_empty() {
                proposals.clear();
                return;
            }
            let dense = followers.refresh(
                &self.followers,
                self.config.width_tiles,
                self.config.height_tiles,
            );
            tables.refresh(self);
            par_map_into(jobs.len(), proposals, |slot| {
                let index = jobs[slot];
                let following = if dense {
                    self.following_unit_in(index, followers)
                } else {
                    self.following_unit(index)
                };
                self.propose_step(index, tables, following)
            });
        }
        #[cfg(target_arch = "wasm32")]
        {
            scratch.tables.refresh(self);
            let Scratch {
                tables, proposals, ..
            } = scratch;
            par_map_into(count, proposals, |turn| {
                let index = *self.lookup.get(&movers[(start + turn) % count])?;
                self.propose_step(index, tables, self.following_unit(index))
            });
        }
    }

    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(crate) fn propose_steps(&self, movers: &[EntityId], start: usize) -> Vec<Option<FastStep>> {
        let mut scratch = Scratch::default();
        self.propose_steps_into(movers, start, &mut scratch);
        (0..movers.len())
            .map(|turn| scratch.take_step(turn))
            .collect()
    }

    /// The single-position-update cases of the sequential waypoint loop. `None`
    /// hands the unit to the sequential path.
    fn propose_step(&self, index: usize, tables: &Tables, following: bool) -> Option<FastStep> {
        let stored = &self.units[index];
        if stored.obstruction.is_some_and(|o| o.blocked >= 16) {
            return None;
        }
        if stored.formation.is_some() {
            return None;
        }
        let mut order = stored.order?;
        let mut state = stored.state;
        if self.arrival_slot_occupied(index) {
            return None;
        }
        let id = state.id;
        let economy = tables.economy(self, state.player);
        let (speed, denominator) = self.movement_speed_with(state.kind, economy);
        let speed = if following && !self.local_retry(id) {
            0
        } else {
            speed
        };
        let mut speed_budget = u128::from(order.speed_carry) + u128::from(speed);
        order.speed_carry = 0;
        let remaining = order.segment_length.saturating_sub(order.travelled);
        let step = u128::from(remaining).min(speed_budget / u128::from(denominator)) as u32;
        if step > 512 {
            return None;
        }
        order.travelled += step;
        speed_budget -= u128::from(step) * u128::from(denominator);
        let arrived = order.travelled >= order.segment_length;
        let position = if arrived {
            order.waypoint
        } else {
            interpolate(order, order.travelled)
        };
        if !self.vassal_position_allowed(id, position) {
            return None;
        }
        let (previous_tile, tile) = (state.position.tile_floor(), position.tile_floor());
        if !self.local_unit_step_clear(index, state.position, position) {
            return None;
        }
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
                .copied()
                .or_else(|| {
                    (order.waypoint != order.destination
                        && order.waypoint.tile_floor() == order.destination.tile_floor())
                    .then_some(order.destination)
                }) {
                Some(next) => Next::Segment(next),
                None => self.propose_map_segment(stored, order)?,
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
            index,
            vision_stale: self.unit_vision_stale(&state, economy),
            bucket: ChunkCoord::from_position(position),
            state,
            order: final_order,
            pops,
            keeps_moving,
            following,
        })
    }

    /// `next_map_segment` for the cases that touch no shared state.
    fn propose_map_segment(
        &self,
        stored: &crate::game_world::StoredUnit,
        order: MovementOrder,
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
        if needs_search {
            // Only the fair per-player pass searches. A mover without a
            // continuation waits even when budget and slots remain available.
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
        // Movement never inserts/removes unit slots. Keep a defensive check
        // for callers retaining a proposal across a separate world mutation.
        let index = step.index;
        if self.units.get(index).is_none_or(|unit| unit.state.id != id) {
            return false;
        }
        if step.following != self.following_unit(index)
            || self.arrival_slot_occupied(index)
            || !self.local_unit_step_clear(
                index,
                self.units[index].state.position,
                step.state.position,
            )
        {
            return false;
        }
        let stored = &mut self.units[index];
        let state_changed = stored.state != step.state;
        let position_changed = stored.state.position != step.state.position;
        let previous_tile = stored.state.position.tile_floor();
        if step.state.planning && !stored.state.planning {
            stored.planning_since = self.tick.0;
        }
        stored.state = step.state;
        stored.order = step.order;
        if step.pops > 0 {
            stored.route.pop_front();
        }
        if step.following {
            self.record_unit_obstruction(index);
        }
        self.observe_unit_progress(index);
        // Wildlife's unit cells follow every step, before vision, as in the
        // sequential path.
        if position_changed {
            self.track_unit_step(id, previous_tile, &step.state);
        }
        if step.vision_stale {
            self.update_unit_vision(id);
        }
        if self.units[index].bucket != step.bucket {
            self.move_bucket(index, step.bucket);
        }
        if step.keeps_moving {
            still_moving.push(id);
        }
        if state_changed {
            changed.push(step.state.id);
        }
        true
    }
}

#[cfg(test)]
mod tests;
