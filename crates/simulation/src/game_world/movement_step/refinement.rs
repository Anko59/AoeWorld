//! Fixed-size quarter-tile refinement around unit obstruction; no global search slot.
use crate::{GameWorld, MovementOrder};
use aoe_core::{TileCoord, WorldPosition};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

const RADIUS: i32 = 8;
const STEP: i32 = 256;

fn grid(p: WorldPosition) -> TileCoord {
    TileCoord::new(p.x.div_euclid(STEP), p.y.div_euclid(STEP))
}
const SIDE: usize = 17;
const CELLS: usize = SIDE * SIDE;
const EXITS: usize = 16;

fn distance(a: WorldPosition, b: WorldPosition) -> u32 {
    crate::game_path::segment_length(
        i64::from(a.x) - i64::from(b.x),
        i64::from(a.y) - i64::from(b.y),
    )
}

fn trace_path(
    mut at: usize,
    source: usize,
    previous: &[usize],
    point: &impl Fn(usize) -> Option<WorldPosition>,
    goal: Option<WorldPosition>,
) -> Option<VecDeque<WorldPosition>> {
    let mut path = VecDeque::new();
    if let Some(goal) = goal
        && point(at)? != goal
    {
        path.push_back(goal);
    }
    while at != source {
        path.push_front(point(at)?);
        at = previous[at];
    }
    Some(path)
}

impl GameWorld {
    /// Rejoin at most four retained route tiles ahead. Flat ground has the
    /// same implicit tile route. The continuation beyond the join stays intact.
    pub(in crate::game_world) fn refine_unit_route(
        &mut self,
        index: usize,
        order: &mut MovementOrder,
    ) -> bool {
        let from = self.units[index].state.position;
        let start = from.tile_floor();
        let stationary_only = self.has_work_task(self.units[index].state.id);
        let mut goal = order.waypoint;
        let mut pops = 0;
        let bypass = order.waypoint != order.destination
            && self.position_occupied(index, order.waypoint, stationary_only);
        let short_join =
            goal.tile_floor().x.abs_diff(start.x) > 1 || goal.tile_floor().y.abs_diff(start.y) > 1;
        for (n, &point) in self.units[index].route.iter().take(4).enumerate() {
            let tile = point.tile_floor();
            if tile.x.abs_diff(start.x) > 1 || tile.y.abs_diff(start.y) > 1 {
                break;
            }
            if !self.position_occupied(index, point, stationary_only)
                && (!stationary_only
                    || !self.destination_point_reserved(point, self.units[index].state.id))
            {
                goal = point;
                pops = n + 1;
            }
        }
        if self.units[index].route.is_empty() && !self.has_route_navigation() {
            for _ in 0..4 {
                if goal.tile_floor() == order.target_tile {
                    break;
                }
                let next =
                    crate::game_path::next_waypoint(goal, order.target_tile, order.destination);
                if next.tile_floor().x.abs_diff(start.x) > 1
                    || next.tile_floor().y.abs_diff(start.y) > 1
                {
                    break;
                }
                goal = next;
            }
        }
        let local = if (short_join || bypass) && pops == 0 {
            self.local_free_arrival_route(index, goal)
        } else {
            self.local_unit_route(index, goal)
        };
        if short_join
            && pops == 0
            && self.units[index]
                .obstruction
                .is_some_and(|o| o.blocked >= 48)
            && local
                .as_ref()
                .and_then(|p| p.back())
                .is_none_or(|&at| distance(at, goal).saturating_add(64) >= distance(from, goal))
            && self.refine_lateral_escape(index, order)
        {
            return true;
        }
        let Some(mut path) = local else {
            return false;
        };
        let Some(waypoint) = path.pop_front() else {
            return false;
        };
        // Repeated transient obstruction cannot grow retained detours forever.
        let retains_waypoint = short_join
            && !bypass
            && pops == 0
            && path.back().copied().unwrap_or(waypoint) != order.waypoint;
        if self.units[index].route.len().saturating_sub(pops)
            + path.len()
            + usize::from(retains_waypoint)
            > 512
        {
            return false;
        }
        for _ in 0..pops {
            self.units[index].route.pop_front();
        }
        if retains_waypoint {
            path.push_back(order.waypoint);
        }
        path.append(&mut self.units[index].route);
        self.units[index].route = path;
        order.origin = from;
        order.waypoint = waypoint;
        order.travelled = 0;
        order.segment_length = crate::game_path::segment_length(
            i64::from(waypoint.x) - i64::from(from.x),
            i64::from(waypoint.y) - i64::from(from.y),
        );
        true
    }

    /// A bounded window can have no forward exit although the static route
    /// can go around the obstruction from another side. Remembered lateral
    /// yielding leaves that pocket before requesting a fresh coarse route.
    fn refine_lateral_escape(&mut self, index: usize, order: &mut MovementOrder) -> bool {
        let from = self.units[index].state.position;
        let dx = i64::from(order.waypoint.x) - i64::from(from.x);
        let dy = i64::from(order.waypoint.y) - i64::from(from.y);
        let length = i64::from(distance(from, order.waypoint));
        let side = i64::from(self.units[index].obstruction.map_or(1, |o| o.side));
        for side in [side, -side] {
            let lateral = (-dy * side, dx * side);
            let goal = WorldPosition::new(
                from.x.saturating_add((lateral.0 * 1536 / length) as i32),
                from.y.saturating_add((lateral.1 * 1536 / length) as i32),
            );
            let Some(mut path) = self.local_free_arrival_route(index, goal) else {
                continue;
            };
            let Some(end) = path.back().copied() else {
                continue;
            };
            let progress = (i64::from(end.x) - i64::from(from.x)) * lateral.0
                + (i64::from(end.y) - i64::from(from.y)) * lateral.1;
            if progress < length * 512 {
                continue;
            }
            let Some(waypoint) = path.pop_front() else {
                continue;
            };
            self.clear_planner(index);
            self.units[index].route = path;
            self.units[index].fast_route_tried = false;
            self.units[index].planning_since = self.tick.0;
            order.origin = from;
            order.waypoint = waypoint;
            order.travelled = 0;
            order.segment_length = distance(from, waypoint);
            return true;
        }
        false
    }

    /// At most 306 expansions (289 quarter-tile points, exact start and 16 corner exits), with
    /// ten normal edges each and sixteen extra source edges for work approaches.
    /// A world shares at most four searches per tick; blocked actors request
    /// a retry on their deterministic eight-tick cadence. Each edge uses at
    /// most five half-tile probes through the bounded fine collision cells.
    /// The exact goal can be reached from any adjacent cell,
    /// including a detour back into the start tile, without rounding a slot.
    pub(crate) fn local_unit_route(
        &mut self,
        index: usize,
        goal: WorldPosition,
    ) -> Option<VecDeque<WorldPosition>> {
        let reserve =
            u32::from(self.units[index].formation.is_none() && !self.formations.is_empty()) * 2;
        if self.local_refinement_left <= reserve {
            return None;
        }
        self.local_refinement_left -= 1;
        self.local_search(index, goal, false)
    }

    pub(super) fn local_free_arrival_route(
        &mut self,
        index: usize,
        goal: WorldPosition,
    ) -> Option<VecDeque<WorldPosition>> {
        let reserve =
            u32::from(self.units[index].formation.is_none() && !self.formations.is_empty()) * 2;
        if self.local_refinement_left <= reserve {
            return None;
        }
        self.local_refinement_left -= 1;
        self.local_search(index, goal, true)
    }

    fn local_search(
        &self,
        index: usize,
        goal: WorldPosition,
        free_arrival: bool,
    ) -> Option<VecDeque<WorldPosition>> {
        let _span = self
            .planning_probe
            .as_ref()
            .and_then(|p| p.movement_span("local_refinement"));
        let unit = self.units[index].state;
        let start = grid(unit.position);
        let start_tile = unit.position.tile_floor();
        let stationary_only = !free_arrival && self.has_work_task(unit.id);
        let slot = |tile: TileCoord| {
            let x = tile.x - start.x + RADIUS;
            let y = tile.y - start.y + RADIUS;
            ((0..SIDE as i32).contains(&x) && (0..SIDE as i32).contains(&y))
                .then(|| y as usize * SIDE + x as usize)
        };
        let tile = |at: usize| {
            TileCoord::new(
                start.x + (at % SIDE) as i32 - RADIUS,
                start.y + (at / SIDE) as i32 - RADIUS,
            )
        };
        // A rejoin ranks this fixed window against the actual distant anchor.
        // Clipping the anchor to a neighbouring tile can put the objective
        // inside a building and repeatedly choose the same obstructed pocket.
        if !free_arrival {
            slot(grid(goal))?;
        }
        if !self.config.valid_ground_position(goal)
            || (!free_arrival && self.position_occupied(index, goal, stationary_only))
        {
            return None;
        }
        let source = CELLS;
        let point = |at: usize| {
            if at == source {
                Some(unit.position)
            } else if at > source {
                // A work approach beside a building can have every adjacent
                // tile-center edge blocked, yet safely pass just outside its
                // corner. Keep the exact first waypoint instead of snapping it
                // into an occupied center or cutting a blocked diagonal.
                let exit = at - source - 1;
                let corner = exit / 4;
                let side = exit % 4;
                Some(WorldPosition::new(
                    (start_tile.x + (corner % 2) as i32) * 1024
                        + if side.is_multiple_of(2) { -1 } else { 1 },
                    (start_tile.y + (corner / 2) as i32) * 1024
                        + if side / 2 == 0 { -1 } else { 1 },
                ))
            } else {
                Some(WorldPosition::new(tile(at).x * STEP, tile(at).y * STEP))
            }
        };
        let mut occupied = [None; CELLS];
        let mut busy = |point: TileCoord| -> bool {
            let Some(at) = slot(point) else {
                return true;
            };
            *occupied[at].get_or_insert_with(|| {
                let p = WorldPosition::new(point.x * STEP, point.y * STEP);
                !self.config.valid_ground_position(p)
                    || self.position_occupied(index, p, stationary_only)
                    || stationary_only && self.destination_point_reserved(p, unit.id)
            })
        };
        let clear = |from: WorldPosition, to: WorldPosition| {
            self.unit_step_clear(unit.kind, from.tile_floor(), to.tile_floor())
                .unwrap_or(false)
                && self.building_step_clear(from.tile_floor(), to.tile_floor())
                && self.clear_edge(index, from, to, stationary_only)
        };
        let mut cost = [u32::MAX; CELLS + 1 + EXITS];
        let mut previous = [usize::MAX; CELLS + 1 + EXITS];
        let mut closed = [false; CELLS + 1 + EXITS];
        cost[source] = 0;
        let h = distance(unit.position, goal);
        let mut frontier = BinaryHeap::from([Reverse((h, h, 0, source))]);
        // A body can occupy a legal fractional pocket with no free lattice
        // point. Its exact source is a reachable arrival too, after the caller
        // has established sustained obstruction. Work/attack jobs never use it.
        let near_source = free_arrival
            && self.units[index]
                .obstruction
                .is_some_and(|o| o.blocked >= 48)
            && distance(unit.position, goal) <= 1536
            && !self.position_occupied(index, unit.position, false)
            && !self.destination_point_reserved(unit.position, unit.id);
        let mut nearest = near_source.then(|| {
            let x = i64::from(unit.position.x) - i64::from(goal.x);
            let y = i64::from(unit.position.y) - i64::from(goal.y);
            (x * x + y * y, 0, start.y, start.x, source)
        });
        while let Some(Reverse((_, _, g, at))) = frontier.pop() {
            if closed[at] || g != cost[at] {
                continue;
            }
            closed[at] = true;
            let from = point(at)?;
            let current = grid(from);
            if free_arrival && at < CELLS && !self.destination_point_reserved(from, unit.id) {
                let x = i64::from(from.x) - i64::from(goal.x);
                let y = i64::from(from.y) - i64::from(goal.y);
                // Exact-goal distance is the optimal arrival. Otherwise inspect
                // the whole bounded window for the closest reachable free spot;
                // the first reachable point can sit outside the arrival crowd.
                if x == 0 && y == 0 {
                    return trace_path(at, source, &previous, &point, None);
                }
                let key = (x * x + y * y, g, current.y, current.x, at);
                if nearest.is_none_or(|best| key < best) {
                    nearest = Some(key);
                }
            }
            if !free_arrival
                && current.x.abs_diff(grid(goal).x) <= 4
                && current.y.abs_diff(grid(goal).y) <= 4
                && clear(from, goal)
            {
                return trace_path(at, source, &previous, &point, Some(goal));
            }
            for (dx, dy) in [
                (0, 0),
                (-1, 0),
                (0, -1),
                (1, 0),
                (0, 1),
                (-1, -1),
                (1, -1),
                (-1, 1),
                (1, 1),
            ] {
                let next = TileCoord::new(current.x + dx, current.y + dy);
                let Some(n) = slot(next) else {
                    continue;
                };
                let to = point(n)?;
                let next_cost = g + distance(from, to);
                if closed[n] || next_cost >= cost[n] || busy(next) || !clear(from, to) {
                    continue;
                }
                cost[n] = next_cost;
                previous[n] = at;
                let h = distance(to, goal);
                frontier.push(Reverse((next_cost + h, h, next_cost, n)));
            }
            // A resumed builder can be between centers beside the TC annex.
            // Include its fine corner exits even when a long detour back to
            // a tile center exists, rather than forcing that detour first.
            if stationary_only
                && at == source
                && (frontier.is_empty() || self.economy.builders.contains_key(&unit.id))
            {
                for at in source + 1..=source + EXITS {
                    let to = point(at)?;
                    if self.config.valid_ground_position(to)
                        && !self.position_occupied(index, to, stationary_only)
                        && clear(unit.position, to)
                    {
                        let g = distance(unit.position, to);
                        cost[at] = g;
                        previous[at] = source;
                        let h = distance(to, goal);
                        frontier.push(Reverse((g + h, h, g, at)));
                    }
                }
            }
        }
        nearest.and_then(|(_, _, _, _, at)| {
            if at == source {
                Some(VecDeque::from([unit.position]))
            } else {
                trace_path(at, source, &previous, &point, None)
            }
        })
    }
}
