//! Fixed-size tile refinement around unit obstruction; no global search slot.
use crate::{GameWorld, MovementOrder};
use aoe_core::{TileCoord, WorldPosition};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

const RADIUS: i32 = 8;
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
    pub(super) fn refine_unit_route(&mut self, index: usize, order: &mut MovementOrder) -> bool {
        let from = self.units[index].state.position;
        let start = from.tile_floor();
        let stationary_only = self.has_work_task(self.units[index].state.id);
        let mut goal = order.waypoint;
        let mut pops = 0;
        for (n, &tile) in self.units[index].route.iter().take(4).enumerate() {
            if tile.x.abs_diff(start.x) > 6 || tile.y.abs_diff(start.y) > 6 {
                break;
            }
            if let Ok(point) = WorldPosition::from_tile_center(tile)
                && !self.position_occupied(index, point, stationary_only)
                && (!stationary_only || !self.work_spot_busy(tile, self.units[index].state.id))
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
                if next.tile_floor().x.abs_diff(start.x) > 6
                    || next.tile_floor().y.abs_diff(start.y) > 6
                {
                    break;
                }
                goal = next;
            }
        }
        let Some(mut path) = self.local_unit_route(index, goal) else {
            return false;
        };
        let Some(waypoint) = path.pop_front() else {
            return false;
        };
        for _ in 0..pops {
            self.units[index].route.pop_front();
        }
        let mut tiles = path
            .into_iter()
            .map(WorldPosition::tile_floor)
            .collect::<VecDeque<_>>();
        tiles.append(&mut self.units[index].route);
        self.units[index].route = tiles;
        order.origin = from;
        order.waypoint = waypoint;
        order.travelled = 0;
        order.segment_length = crate::game_path::segment_length(
            i64::from(waypoint.x) - i64::from(from.x),
            i64::from(waypoint.y) - i64::from(from.y),
        );
        true
    }

    /// At most 306 expansions (289 centers, exact start and 16 corner exits), with
    /// ten normal edges each and sixteen extra source edges for work approaches.
    /// Separated units retry once per eight ticks;
    /// workers escaping an overlap can retry each tick, once per unit/tick.
    /// Each edge uses at most five half-tile probes of 128 neighbours.
    /// The exact goal can be reached from any adjacent cell,
    /// including a detour back into the start tile, without rounding a slot.
    pub(crate) fn local_unit_route(
        &self,
        index: usize,
        goal: WorldPosition,
    ) -> Option<VecDeque<WorldPosition>> {
        self.local_search(index, goal, false)
    }

    pub(super) fn local_free_arrival_route(
        &self,
        index: usize,
        goal: WorldPosition,
    ) -> Option<VecDeque<WorldPosition>> {
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
        let start = unit.position.tile_floor();
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
        slot(goal.tile_floor())?;
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
                    (start.x + (corner % 2) as i32) * 1024
                        + if side.is_multiple_of(2) { -1 } else { 1 },
                    (start.y + (corner / 2) as i32) * 1024 + if side / 2 == 0 { -1 } else { 1 },
                ))
            } else {
                WorldPosition::from_tile_center(tile(at)).ok()
            }
        };
        let mut occupied = [None; CELLS];
        let mut busy = |point: TileCoord| -> bool {
            let Some(at) = slot(point) else {
                return true;
            };
            *occupied[at].get_or_insert_with(|| {
                !WorldPosition::from_tile_center(point).is_ok_and(|p| {
                    self.config.valid_ground_position(p)
                        && !self.position_occupied(index, p, stationary_only)
                        // A work approach must route around the other workers'
                        // reserved arrivals before they become stationary walls.
                        && (!stationary_only || !self.work_spot_busy(point, unit.id))
                })
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
        let mut nearest = None;
        while let Some(Reverse((_, _, g, at))) = frontier.pop() {
            if closed[at] || g != cost[at] {
                continue;
            }
            closed[at] = true;
            let from = point(at)?;
            let current = from.tile_floor();
            if free_arrival
                && at < CELLS
                && current.x.abs_diff(goal.tile_floor().x) <= 8
                && current.y.abs_diff(goal.tile_floor().y) <= 8
                && !self.work_spot_busy(current, unit.id)
            {
                let x = i64::from(from.x) - i64::from(goal.x);
                let y = i64::from(from.y) - i64::from(goal.y);
                // Accept a reachable nearby slot as soon as A* finds it.
                // Exhausting the whole square to improve an already legal
                // arrival by a fraction of a tile keeps crowds moving.
                if x * x + y * y <= (3 * 1024_i64).pow(2) {
                    return trace_path(at, source, &previous, &point, None);
                }
                let key = (x * x + y * y, g, current.y, current.x, at);
                if nearest.is_none_or(|best| key < best) {
                    nearest = Some(key);
                }
            }
            if !free_arrival
                && current.x.abs_diff(goal.tile_floor().x) <= 1
                && current.y.abs_diff(goal.tile_floor().y) <= 1
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
        nearest.and_then(|(_, _, _, _, at)| trace_path(at, source, &previous, &point, None))
    }
}
