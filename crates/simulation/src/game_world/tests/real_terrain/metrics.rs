use super::*;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

#[derive(Debug)]
pub(super) struct Metrics {
    pub arrived: usize,
    pub last_tick: u32,
    pub ideal_ticks: u32,
    pub max_stuck_ticks: u32,
    pub oscillations: u32,
    pub max_slot_error: i64,
    pub route_work: u64,
    pub max_tick_route_work: u64,
    pub member_work: u64,
    pub path_work: u64,
    pub max_tick_path_work: u64,
    pub collision_work: u64,
}

impl Metrics {
    pub fn assert_works(&self, count: usize) {
        assert_eq!(self.arrived, count, "{self:?}");
        assert!(self.last_tick * 10 <= self.ideal_ticks * 13, "{self:?}");
        assert!(self.max_stuck_ticks <= 60, "{self:?}");
        assert_eq!(self.max_slot_error, 0, "{self:?}");
        assert_eq!(self.oscillations, 0, "{self:?}");
        // One shared bounded route continuation plus a fixed local cursor
        // window per member; the cap does not depend on march distance.
        assert!(
            self.max_tick_path_work <= count as u64 * 128 + 1024,
            "{self:?}"
        );
        assert!(
            self.max_tick_route_work
                <= u64::from(crate::MAX_ROUTE_WORK_PER_TICK)
                    + u64::from(crate::game_movement::MAX_FAST_ROUTE_WORK_PER_TICK),
            "{self:?}"
        );
    }
}

fn distance(a: WorldPosition, b: WorldPosition) -> i64 {
    (i64::from(a.x - b.x).pow(2) + i64::from(a.y - b.y).pow(2)).isqrt()
}

pub(super) fn measure(
    w: &mut GameWorld,
    goals: &[(EntityId, WorldPosition)],
    limit: u32,
    ideal_ticks: u32,
) -> Metrics {
    let start = w.route_scheduler_stats().metrics;
    let mut out = Metrics {
        arrived: 0,
        last_tick: 0,
        ideal_ticks,
        max_stuck_ticks: 0,
        oscillations: 0,
        max_slot_error: 0,
        route_work: 0,
        max_tick_route_work: 0,
        member_work: 0,
        path_work: 0,
        max_tick_path_work: 0,
        collision_work: 0,
    };
    let mut previous = goals
        .iter()
        .map(|&(id, _)| w.unit(id).unwrap().position)
        .collect::<Vec<_>>();
    let grouped = goals
        .iter()
        .map(|&(id, _)| w.formation(id).is_some())
        .collect::<Vec<_>>();
    let mut previous_orders = goals
        .iter()
        .map(|&(id, _)| w.movement_order(id))
        .collect::<Vec<_>>();
    let mut significant = previous.clone();
    let mut directions = vec![(0_i32, 0_i32); goals.len()];
    let mut still = vec![0; goals.len()];
    let mut reported = vec![false; goals.len()];
    let mut peaks = vec![(0, WorldPosition::new(0, 0)); goals.len()];
    let mut reversal_at = vec![None; goals.len()];
    let mut old_work = start.issue_work + start.fair_work + start.lookahead_work;
    let mut old_path_work = start.group_path_work;
    for tick in 1..=limit {
        w.advance();
        out.arrived = 0;
        out.max_slot_error = 0;
        for (n, &(id, goal)) in goals.iter().enumerate() {
            let unit = w.unit(id).unwrap();
            let at = unit.position;
            let (speed, denominator) = w.movement_speed(unit.kind, unit.player);
            assert!(
                distance(at, previous[n]) <= speed.div_ceil(denominator) as i64 + 2,
                "unit {id:?} spent more than one tick at tick {tick}: from={:?} to={at:?} speed={speed}/{denominator} stored_previous={:?} formation={:?} detached={} prior_order={:?} order={:?} active_copies={}",
                previous[n],
                unit.previous_position,
                w.formation(id),
                w.is_detached_member(w.lookup[&id]),
                previous_orders[n],
                w.units[w.lookup[&id]].order,
                w.active_movers.iter().filter(|&&other| other == id).count()
            );
            assert!(
                !grouped[n] || w.formation(id).is_some(),
                "an internal recovery released {id:?} from its original group at tick {tick}"
            );
            let arrived = at == goal && !unit.moving && !unit.planning;
            out.arrived += usize::from(arrived);
            if !arrived && at == previous[n] {
                still[n] += 1;
                out.max_stuck_ticks = out.max_stuck_ticks.max(still[n]);
                if still[n] > peaks[n].0 {
                    peaks[n] = (still[n], at);
                }
                if still[n] == 61 && !reported[n] {
                    reported[n] = true;
                    let index = w.lookup[&id];
                    let stored = &w.units[index];
                    eprintln!(
                        "terrain-stall tick={tick} id={id:?} at={at:?} goal={goal:?} kind={:?} order={:?} obstruction={:?} motion={:?}",
                        unit.kind,
                        stored.order,
                        stored.obstruction,
                        w.formation_motion(index)
                    );
                }
            } else {
                still[n] = 0;
            }
            if distance(at, significant[n]) >= 512 {
                let direction = (at.x - significant[n].x, at.y - significant[n].y);
                let last = directions[n];
                // Oscillation is a repeated >90 degree reversal within three
                // seconds, each after half a tile. A single deliberate turn
                // around an obstacle and one-subunit jitter do not count.
                if i64::from(direction.0) * i64::from(last.0)
                    + i64::from(direction.1) * i64::from(last.1)
                    < 0
                {
                    if reversal_at[n].is_some_and(|last| tick - last <= 60) {
                        out.oscillations += 1;
                        if out.oscillations <= 16 {
                            eprintln!(
                                "terrain-oscillation tick={tick} id={id:?} at={at:?} from_direction={last:?} to_direction={direction:?} motion={:?}",
                                w.formation_motion(w.lookup[&id])
                            );
                        }
                    }
                    reversal_at[n] = Some(tick);
                }
                directions[n] = direction;
                significant[n] = at;
            }
            previous[n] = at;
            previous_orders[n] = w.movement_order(id);
            out.max_slot_error = out.max_slot_error.max(distance(at, goal));
        }
        let metrics = w.route_scheduler_stats().metrics;
        let work = metrics.issue_work + metrics.fair_work + metrics.lookahead_work;
        out.max_tick_route_work = out.max_tick_route_work.max(work - old_work);
        old_work = work;
        out.max_tick_path_work = out
            .max_tick_path_work
            .max(metrics.group_path_work - old_path_work);
        old_path_work = metrics.group_path_work;
        out.last_tick = tick;
        if out.arrived == goals.len() {
            break;
        }
    }
    for (n, &(id, _)) in goals.iter().enumerate() {
        if peaks[n].0 > 60 {
            eprintln!(
                "terrain-peak id={id:?} ticks={} at={:?}",
                peaks[n].0, peaks[n].1
            );
        }
    }
    for &(id, goal) in goals {
        let index = w.lookup[&id];
        let u = &w.units[index];
        if u.state.position != goal {
            eprintln!(
                "terrain-stranded id={id:?} unit={:?} goal={goal:?} order={:?} route={:?} error={:?} obstruction={:?} motion={:?}",
                u.state,
                u.order,
                u.route.iter().take(4).collect::<Vec<_>>(),
                u.last_movement_error,
                u.obstruction,
                w.formation_motion(index)
            );
        }
    }
    let end = w.route_scheduler_stats().metrics;
    out.route_work = old_work - start.issue_work - start.fair_work - start.lookahead_work;
    out.member_work = end.group_member_work - start.group_member_work;
    out.path_work = end.group_path_work - start.group_path_work;
    out.collision_work = end.group_collision_work - start.group_collision_work;
    out
}

/// Independent A* over the physical static edges, without traffic. This
/// oracle is deliberately test-only and not constrained by runtime budgets.
fn shortest(w: &GameWorld, kind: UnitKind, from: TileCoord, goal: TileCoord) -> u64 {
    let heuristic = |p: TileCoord| {
        let x = p.x.abs_diff(goal.x);
        let y = p.y.abs_diff(goal.y);
        u64::from(x.max(y)) * 1024 + u64::from(x.min(y)) * 424
    };
    let mut open = BinaryHeap::from([Reverse((heuristic(from), 0_u64, from))]);
    let mut costs = BTreeMap::from([(from, 0)]);
    while let Some(Reverse((_, cost, tile))) = open.pop() {
        if tile == goal {
            return cost;
        }
        if costs[&tile] != cost {
            continue;
        }
        assert!(
            costs.len() <= 200_000,
            "reference search exceeded fixture bound"
        );
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let next = TileCoord::new(tile.x + dx, tile.y + dy);
                let step = if dx == 0 || dy == 0 { 1024 } else { 1448 };
                let next_cost = cost + step;
                if costs.get(&next).is_some_and(|&old| old <= next_cost) {
                    continue;
                }
                if !w.unit_step_clear(kind, tile, next).unwrap_or(false)
                    || !w.building_step_clear(tile, next)
                    || !w.body_step_clear(kind, center(tile.x, tile.y), center(next.x, next.y))
                {
                    continue;
                }
                costs.insert(next, next_cost);
                open.push(Reverse((next_cost + heuristic(next), next_cost, next)));
            }
        }
    }
    panic!("fixture goal is physically unreachable");
}

pub(super) fn reference_ticks(w: &GameWorld, goals: &[(EntityId, WorldPosition)]) -> u32 {
    let pace = goals
        .iter()
        .map(|&(id, _)| {
            let u = w.unit(id).unwrap();
            w.movement_speed(u.kind, u.player)
        })
        .min_by(|a, b| {
            (u128::from(a.0) * u128::from(b.1)).cmp(&(u128::from(b.0) * u128::from(a.1)))
        })
        .unwrap();
    goals
        .iter()
        .map(|&(id, goal)| {
            let unit = w.unit(id).unwrap();
            let cost = shortest(w, unit.kind, unit.position.tile_floor(), goal.tile_floor())
                + distance(goal, center(goal.tile_floor().x, goal.tile_floor().y)) as u64;
            (cost * pace.1).div_ceil(pace.0) as u32
        })
        .max()
        .unwrap()
}
