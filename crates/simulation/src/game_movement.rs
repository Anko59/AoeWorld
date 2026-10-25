use crate::{
    GameWorld, GameWorldError, MAX_ROUTE_WORK_PER_ORDER, MAX_ROUTE_WORK_PER_TICK, MovementOrder,
    game_path::next_waypoint,
    game_path::segment_length,
    game_world::{facing_for, interpolate},
};
use aoe_core::{ChunkCoord, EntityId, WorldPosition};
use aoe_map::{RoutePlanner, RoutePlannerPoll};
use std::collections::VecDeque;

const MAX_WAYPOINTS_PER_TICK: usize = 4_096;
/// World-wide bound on compact proven route continuations; each player also
/// has its own share (`route_scheduler`), so this bound is memory only.
pub(crate) const MAX_ACTIVE_ROUTE_PLANNERS: usize = 4_096;
/// World-wide bound on planners retaining sparse search frontiers. A player
/// holds at most `MAX_ROUTE_SEARCHES_PER_PLAYER` near and as many far ones,
/// so 100 seats fit.
pub(crate) const MAX_ACTIVE_ROUTE_SEARCHES: usize = 400;
/// Work of one order's fast attempt: a route proven with this much work (or
/// a destination proven unreachable) is answered at once, with no search slot
/// and no queue. Nearly every order of a real match fits (the gather and
/// return trips of villagers are a few tiles long; a river or forest detour
/// of a few dozen tiles takes 4,096 to 16,384). It is also what a near trip
/// to ground it cannot reach costs before it is given up (`approach_if_hard`),
/// and with armies in the field 16,384 saturated the world's fast budget.
pub(crate) const FAST_ROUTE_WORK: u32 = 8_192;
/// World-wide work of fast attempts per tick, separate from the fair budget
/// that serves the searches holding slots, so slow searches never delay a
/// short order and short orders never starve a slow search.
pub(crate) const MAX_FAST_ROUTE_WORK_PER_TICK: u32 = 65_536;
/// One player's share of that budget per tick: a client that floods orders
/// spends its own share, never the other players' fast attempts.
pub(crate) const MAX_FAST_ROUTE_WORK_PER_PLAYER: u32 = 2 * FAST_ROUTE_WORK;

impl GameWorld {
    /// Moves every active mover once and returns the ids it touched, in
    /// arbitrary order; the tick collation sorts and fetches final states.
    pub(crate) fn advance_movement(&mut self) -> Vec<EntityId> {
        // Waiting units search first, fairly per player, so a route found now
        // is walked this tick; walking units plan ahead from what is left.
        let mut changed = Vec::new();
        // One tick budget in all: what issued orders left of it (at least the
        // reserve) is shared here; orders issued later this tick get none.
        let fair_budget = std::mem::take(&mut self.planning_budget);
        let lookahead_budget = self.serve_route_searches(fair_budget, &mut changed);
        let movers = std::mem::take(&mut self.active_movers);
        let mover_count = movers.len();
        let start = if mover_count == 0 {
            0
        } else {
            self.planning_cursor % mover_count
        };
        self.planning_cursor = if mover_count == 0 {
            0
        } else {
            (start + 1) % mover_count
        };
        let mut still_moving = Vec::with_capacity(movers.len());
        changed.reserve(movers.len());
        // Pure proposals in parallel, then the shared writes in rotation
        // order; a proposal whose assumption fails runs the sequential path.
        self.begin_vision_batch();
        let mut proposals = self.propose_steps(&movers, start);
        for turn in 0..mover_count {
            if let Some(step) = proposals[turn].take()
                && self.apply_step(step, &mut still_moving, &mut changed)
            {
                continue;
            }
            // The rotation by the planning cursor, without copying the list.
            let id = movers[(start + turn) % mover_count];
            let Some(&index) = self.lookup.get(&id) else {
                continue;
            };
            if self.advance_formation_anchor(index, &mut still_moving) {
                continue;
            }
            let Some(mut order) = self.units[index].order else {
                continue;
            };
            self.units[index].state.previous_position = self.units[index].state.position;
            let unit = self.units[index].state;
            let (speed, denominator) = self.movement_speed(unit.kind, unit.player);
            let speed = if self.following_unit(index) { 0 } else { speed };
            let mut speed_budget = u128::from(order.speed_carry) + u128::from(speed);
            order.speed_carry = 0;
            let mut current_position = self.units[index].state.position;
            let mut finished = false;

            for _ in 0..MAX_WAYPOINTS_PER_TICK {
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
                let previous_tile = current_position.tile_floor();
                let tile = position.tile_floor();
                let terrain_ok =
                    match self.unit_step_clear(self.units[index].state.kind, previous_tile, tile) {
                        Ok(terrain_ok) => terrain_ok,
                        Err(error) => {
                            self.clear_planner(index);
                            self.units[index].state.moving = false;
                            self.units[index].state.planning = false;
                            self.units[index].order = None;
                            self.units[index].last_movement_error =
                                Some(GameWorldError::Environment(error));
                            finished = true;
                            break;
                        }
                    };
                if !terrain_ok || !self.building_step_clear(previous_tile, tile) {
                    // Collision can change after a segment was refined. Keep
                    // the order and replan from the last valid fixed position.
                    self.clear_planner(index);
                    self.units[index].route.clear();
                    self.units[index].state.moving = false;
                    self.units[index].state.planning = true;
                    self.units[index].planning_since = self.tick.0;
                    self.units[index].fast_route_tried = false;
                    self.units[index].last_movement_error = None;
                    self.units[index].order = Some(MovementOrder {
                        origin: current_position,
                        waypoint: current_position,
                        segment_length: 1,
                        travelled: 0,
                        speed_carry: 0,
                        ..order
                    });
                    still_moving.push(id);
                    finished = true;
                    break;
                }
                self.units[index].state.position = position;
                let stepped = self.units[index].state;
                self.track_unit_step(id, previous_tile, &stepped);
                self.update_unit_vision(id);
                if self.units[index].bucket != ChunkCoord::from_position(position) {
                    self.move_bucket(index, ChunkCoord::from_position(position));
                }
                current_position = position;
                if !arrived {
                    order.speed_carry = speed_budget as u64;
                    self.units[index].state.moving = true;
                    self.units[index].state.planning = false;
                    self.units[index].order = Some(order);
                    still_moving.push(id);
                    finished = true;
                    break;
                }
                if order.waypoint == order.destination && self.units[index].route.is_empty() {
                    self.clear_planner(index);
                    self.units[index].state.moving = false;
                    self.units[index].state.planning = false;
                    self.units[index].order = None;
                    finished = true;
                    break;
                }
                let next = self.units[index]
                    .route
                    .pop_front()
                    .and_then(|tile| WorldPosition::from_tile_center(tile).ok())
                    .map(SegmentAdvance::Next)
                    .or_else(|| {
                        (order.waypoint != order.destination
                            && order.waypoint.tile_floor() == order.destination.tile_floor())
                        .then_some(SegmentAdvance::Next(order.destination))
                    })
                    .unwrap_or_else(|| self.next_map_segment(index, order));
                match next {
                    SegmentAdvance::Planning => {
                        // A deferred planner has no available path distance this
                        // tick. Keep only the fractional remainder, never a
                        // whole movement credit that could cause a later dash.
                        order.speed_carry = (speed_budget % u128::from(denominator)) as u64;
                        if !self.units[index].state.planning {
                            self.units[index].planning_since = self.tick.0;
                        }
                        self.units[index].state.moving = false;
                        self.units[index].state.planning = true;
                        self.units[index].order = Some(order);
                        still_moving.push(id);
                        finished = true;
                        break;
                    }
                    SegmentAdvance::Stopped(error) => {
                        if error == Some(GameWorldError::Unreachable) {
                            self.remember_unreachable(index, order.target_tile);
                        }
                        self.clear_planner(index);
                        self.units[index].state.moving = false;
                        self.units[index].state.planning = false;
                        self.units[index].order = None;
                        self.units[index].last_movement_error = error;
                        finished = true;
                        break;
                    }
                    SegmentAdvance::Next(next) => {
                        let dx = i64::from(next.x) - i64::from(order.waypoint.x);
                        let dy = i64::from(next.y) - i64::from(order.waypoint.y);
                        order = MovementOrder {
                            origin: order.waypoint,
                            waypoint: next,
                            segment_length: segment_length(dx, dy),
                            travelled: 0,
                            speed_carry: 0,
                            ..order
                        };
                        self.units[index].state.facing =
                            facing_for(dx, dy, self.units[index].state.facing);
                        if speed_budget < u128::from(denominator) {
                            order.speed_carry = speed_budget as u64;
                            self.units[index].state.moving = true;
                            self.units[index].state.planning = false;
                            self.units[index].order = Some(order);
                            still_moving.push(id);
                            finished = true;
                            break;
                        }
                    }
                }
            }
            if !finished {
                // The route is bounded even for deliberately tiny custom
                // segments and very large custom speeds. Unused whole credits
                // are discarded at the bound; only the fractional remainder
                // is carried into the next tick.
                order.speed_carry = (speed_budget % u128::from(denominator)) as u64;
                self.units[index].state.moving = true;
                self.units[index].state.planning = false;
                self.units[index].order = Some(order);
                still_moving.push(id);
            }
            if !self.economy.gatherers.contains_key(&id)
                && self.hunting_target(id).is_none()
                && !self.combat.attackers.contains_key(&id)
                && !self.economy.builders.contains_key(&id)
            {
                let unit = &mut self.units[index].state;
                unit.task = if unit.moving || unit.planning {
                    crate::UnitTask::Moving
                } else if unit.carried.is_some() {
                    crate::UnitTask::Carrying
                } else {
                    crate::UnitTask::Idle
                };
            }
            changed.push(self.units[index].state.id);
        }
        still_moving.sort_unstable();
        self.active_movers = still_moving;
        self.advance_formations(&mut changed);
        self.flush_vision_batch();
        self.engage_formations(&mut changed);
        self.plan_routes_ahead(&movers, start, lookahead_budget);
        self.resume_queued_moves(&mut changed);
        self.tick.0 = self.tick.0.saturating_add(1);
        changed
    }

    pub(crate) fn next_map_segment(
        &mut self,
        index: usize,
        order: MovementOrder,
    ) -> SegmentAdvance {
        if !self.has_route_navigation() {
            return (order.waypoint != order.destination)
                .then(|| next_waypoint(order.waypoint, order.target_tile, order.destination))
                .map_or(SegmentAdvance::Stopped(None), SegmentAdvance::Next);
        }
        // A planner with planned segments left emits one without searching.
        // Searches are never run inline: `serve_route_searches` shares them
        // fairly between players before the units move.
        let needs_search = self.units[index]
            .planner
            .as_ref()
            .is_none_or(|planner| !planner.has_route_continuation());
        if needs_search {
            return SegmentAdvance::Planning;
        }
        let budget = 0;
        let mut planner = self.take_planner(index).unwrap_or_else(|| {
            RoutePlanner::new(
                order.waypoint.tile_floor(),
                order.target_tile,
                MAX_ROUTE_WORK_PER_ORDER,
            )
        });
        let work_before = planner.work();
        let result = self.poll_world_route_planner(
            &mut planner,
            budget,
            &|| false,
            self.units[index].state.kind,
        );
        self.refund_unused_planning(budget, work_before, &planner);
        match result {
            RoutePlannerPoll::Path(path) => {
                let mut route = VecDeque::from(path.tiles);
                if route.pop_front() != Some(order.waypoint.tile_floor()) {
                    return SegmentAdvance::Stopped(Some(GameWorldError::InvalidPosition));
                }
                let next = route
                    .pop_front()
                    .and_then(|tile| WorldPosition::from_tile_center(tile).ok());
                self.units[index].route = route;
                // Keep the certified endpoint's terminal verdict until the
                // unit consumes this last segment and asks for continuation.
                if !planner.is_terminal()
                    || planner.has_route_continuation()
                    || planner.certified_destination().is_some()
                {
                    self.store_planner(index, Some(planner));
                }
                next.map_or(
                    SegmentAdvance::Stopped(Some(GameWorldError::InvalidPosition)),
                    SegmentAdvance::Next,
                )
            }
            RoutePlannerPoll::Pending => {
                self.store_planner(index, Some(planner));
                SegmentAdvance::Planning
            }
            RoutePlannerPoll::Complete => {
                SegmentAdvance::Stopped(Some(GameWorldError::InvalidTerrain))
            }
            RoutePlannerPoll::InvalidDestination => {
                SegmentAdvance::Stopped(Some(GameWorldError::InvalidPosition))
            }
            RoutePlannerPoll::Unreachable => {
                SegmentAdvance::Stopped(Some(GameWorldError::Unreachable))
            }
            RoutePlannerPoll::SearchLimit => {
                SegmentAdvance::Stopped(Some(GameWorldError::PathBudgetExceeded))
            }
            RoutePlannerPoll::Environment(error) => {
                SegmentAdvance::Stopped(Some(GameWorldError::Environment(error)))
            }
        }
    }

    /// Plans the leg after the one each moving unit is walking, from what
    /// the waiting units left of the tick budget, so a walking unit does not
    /// pause between legs. A search that fails here is reported when the unit
    /// has used the segments already planned.
    fn plan_routes_ahead(&mut self, movers: &[EntityId], start: usize, mut budget: u32) {
        let wanting = (0..movers.len())
            .map(|turn| movers[(start + turn) % movers.len()])
            .filter_map(|id| self.lookup.get(&id).copied())
            .filter(|&index| {
                self.units[index].order.is_some()
                    && self.units[index]
                        .planner
                        .as_ref()
                        .is_some_and(RoutePlanner::wants_lookahead)
            })
            .collect::<Vec<_>>();
        for (turn, &index) in wanting.iter().enumerate() {
            if budget == 0 {
                break;
            }
            if self.planner_capacity_exhausted(index) || self.lookahead_jumps_queue(index) {
                continue;
            }
            let share = (budget / (wanting.len() - turn) as u32).clamp(1, MAX_ROUTE_WORK_PER_ORDER);
            let Some(mut planner) = self.take_planner(index) else {
                continue;
            };
            let work_before = planner.work();
            let kind = self.units[index].state.kind;
            let certified_before = planner.certified_destination();
            self.plan_world_route_ahead(&mut planner, share, &|| false, kind);
            if certified_before.is_none() && planner.certified_destination().is_some() {
                self.route_metrics
                    .sample("certified-nearest", kind, &planner);
            }
            let spent = planner.work().saturating_sub(work_before);
            self.route_metrics.lookahead_work += u64::from(spent);
            budget -= spent.min(budget);
            self.store_planner(index, Some(planner));
        }
    }

    /// A planner rarely needs its whole share for one leg; what it did not
    /// spend returns to the tick budget for the movers after it.
    fn refund_unused_planning(&mut self, budget: u32, work_before: u32, planner: &RoutePlanner) {
        let spent = planner.work().saturating_sub(work_before);
        self.planning_budget = self
            .planning_budget
            .saturating_add(budget.saturating_sub(spent))
            .min(MAX_ROUTE_WORK_PER_TICK);
    }

    /// A new leg search planned ahead would take a slot its player's waiting
    /// units queue for.
    fn lookahead_jumps_queue(&self, index: usize) -> bool {
        self.units[index].planner.as_ref().is_some_and(|planner| {
            !planner.requires_search_slot()
                && self.route_queued.contains(&(
                    self.units[index].state.player,
                    crate::game_world::far_route(planner.origin(), planner.destination()),
                ))
        })
    }

    fn planner_capacity_exhausted(&self, index: usize) -> bool {
        let own_planner = usize::from(self.units[index].planner.is_some());
        let own_search = usize::from(
            self.units[index]
                .planner
                .as_ref()
                .is_some_and(RoutePlanner::requires_search_slot),
        );
        let far = self.units[index].planner.as_ref().is_some_and(|planner| {
            crate::game_world::far_route(planner.origin(), planner.destination())
        });
        self.route_capacity_exhausted(self.units[index].state.player, own_planner, own_search, far)
    }

    pub(crate) fn clear_planner(&mut self, index: usize) {
        let _ = self.take_planner(index);
    }

    pub(crate) fn store_planner(&mut self, index: usize, planner: Option<RoutePlanner>) {
        self.clear_planner(index);
        if let Some(planner) = &planner {
            self.active_planner_count = self.active_planner_count.saturating_add(1);
            if planner.requires_search_slot() {
                self.active_route_searches = self.active_route_searches.saturating_add(1);
            }
            self.count_route_slots(self.units[index].state.player, planner, true);
        }
        self.units[index].planner = planner;
    }

    pub(crate) fn take_planner(&mut self, index: usize) -> Option<RoutePlanner> {
        let planner = self.units[index].planner.take()?;
        self.active_planner_count = self.active_planner_count.saturating_sub(1);
        if planner.requires_search_slot() {
            self.active_route_searches = self.active_route_searches.saturating_sub(1);
        }
        self.count_route_slots(self.units[index].state.player, &planner, false);
        Some(planner)
    }
}

pub(crate) enum SegmentAdvance {
    Next(WorldPosition),
    Planning,
    Stopped(Option<GameWorldError>),
}
