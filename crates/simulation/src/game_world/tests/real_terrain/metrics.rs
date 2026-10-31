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
    pub max_speed_excess: i64,
    pub lost_group_ticks: u64,
}

impl Metrics {
    pub fn assert_works(&self, count: usize) {
        self.assert_arrival_and_work(count);
        assert!(self.max_stuck_ticks <= 60, "{self:?}");
        assert_eq!(self.oscillations, 0, "{self:?}");
    }

    pub fn assert_arrival_and_work(&self, count: usize) {
        assert_eq!(self.max_speed_excess, 0, "{self:?}");
        assert_eq!(self.lost_group_ticks, 0, "{self:?}");
        assert_eq!(self.arrived, count, "{self:?}");
        assert!(self.last_tick * 10 <= self.ideal_ticks * 13, "{self:?}");
        assert_eq!(self.max_slot_error, 0, "{self:?}");
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

/// Starts before command issue. Unknown General slots remain pending units;
/// admission and its cadence cannot disappear from the observations.
pub(super) struct Tracker {
    pub starts: Vec<(EntityId, WorldPosition)>,
    goals: Vec<Option<WorldPosition>>,
    previous: Vec<WorldPosition>,
    grouped: Vec<bool>,
    significant: Vec<WorldPosition>,
    directions: Vec<(i32, i32)>,
    still: Vec<u32>,
    reversal_at: Vec<Option<u32>>,
    start_work: [u64; 4],
    old_work: u64,
    old_path: u64,
    out: Metrics,
}

impl Tracker {
    pub fn new(w: &GameWorld, ids: &[EntityId]) -> Self {
        let previous = ids
            .iter()
            .map(|&id| w.unit(id).unwrap().position)
            .collect::<Vec<_>>();
        let m = w.route_scheduler_stats().metrics;
        let work = m.issue_work + m.fair_work + m.lookahead_work;
        Self {
            starts: ids.iter().copied().zip(previous.iter().copied()).collect(),
            goals: vec![None; ids.len()],
            grouped: ids.iter().map(|&id| w.formation(id).is_some()).collect(),
            significant: previous.clone(),
            previous,
            directions: vec![(0, 0); ids.len()],
            still: vec![0; ids.len()],
            reversal_at: vec![None; ids.len()],
            start_work: [
                work,
                m.group_member_work,
                m.group_path_work,
                m.group_collision_work,
            ],
            old_work: work,
            old_path: m.group_path_work,
            out: Metrics {
                arrived: 0,
                last_tick: 0,
                ideal_ticks: 0,
                max_stuck_ticks: 0,
                oscillations: 0,
                max_slot_error: 0,
                route_work: 0,
                max_tick_route_work: 0,
                member_work: 0,
                path_work: 0,
                max_tick_path_work: 0,
                collision_work: 0,
                max_speed_excess: 0,
                lost_group_ticks: 0,
            },
        }
    }

    pub fn attach(&mut self, offered: &[Option<WorldPosition>]) {
        assert_eq!(offered.len(), self.goals.len());
        for (stored, &goal) in self.goals.iter_mut().zip(offered) {
            if let Some(goal) = goal {
                if let Some(old) = *stored {
                    assert_eq!(old, goal, "a recovery changed a final slot");
                }
                *stored = Some(goal);
            }
        }
    }

    pub fn goals(&self) -> Vec<(EntityId, WorldPosition)> {
        self.starts
            .iter()
            .zip(&self.goals)
            .map(|(&(id, _), &goal)| {
                (
                    id,
                    goal.expect("every commanded actor must receive its destination"),
                )
            })
            .collect()
    }

    pub fn observe(&mut self, w: &GameWorld, tick: u32) -> bool {
        self.out.arrived = 0;
        self.out.max_slot_error = 0;
        for (n, &(id, _)) in self.starts.iter().enumerate() {
            let unit = w.unit(id).unwrap();
            let at = unit.position;
            let (speed, denominator) = w.movement_speed(unit.kind, unit.player);
            self.out.max_speed_excess = self
                .out
                .max_speed_excess
                .max(distance(at, self.previous[n]) - speed.div_ceil(denominator) as i64 - 2);
            let grouped = w.formation(id).is_some();
            let arrived = self.goals[n] == Some(at) && !unit.moving && !unit.planning;
            self.out.lost_group_ticks += u64::from(!arrived && self.grouped[n] && !grouped);
            self.grouped[n] |= grouped;
            self.out.arrived += usize::from(arrived);
            if !arrived && at == self.previous[n] {
                self.still[n] += 1;
                self.out.max_stuck_ticks = self.out.max_stuck_ticks.max(self.still[n]);
                if self.still[n] == 61 {
                    let index = w.lookup[&id];
                    eprintln!(
                        "terrain-stall tick={tick} id={id:?} at={at:?} goal={:?} kind={:?} order={:?} obstruction={:?} motion={:?}",
                        self.goals[n],
                        unit.kind,
                        w.units[index].order,
                        w.units[index].obstruction,
                        w.formation_motion(index)
                    );
                }
            } else {
                self.still[n] = 0;
            }
            if distance(at, self.significant[n]) >= 512 {
                let direction = (at.x - self.significant[n].x, at.y - self.significant[n].y);
                let last = self.directions[n];
                if i64::from(direction.0) * i64::from(last.0)
                    + i64::from(direction.1) * i64::from(last.1)
                    < 0
                {
                    if self.reversal_at[n].is_some_and(|last| tick - last <= 60) {
                        self.out.oscillations += 1;
                        if self.out.oscillations <= 8 {
                            eprintln!(
                                "terrain-reversal tick={tick} id={id:?} kind={:?} at={at:?} previous={:?} direction={direction:?} prior={last:?} goal={:?}",
                                unit.kind, self.significant[n], self.goals[n]
                            );
                        }
                    }
                    self.reversal_at[n] = Some(tick);
                }
                self.directions[n] = direction;
                self.significant[n] = at;
            }
            self.previous[n] = at;
            if let Some(goal) = self.goals[n] {
                self.out.max_slot_error = self.out.max_slot_error.max(distance(at, goal));
            }
        }
        let m = w.route_scheduler_stats().metrics;
        let work = m.issue_work + m.fair_work + m.lookahead_work;
        self.out.max_tick_route_work = self.out.max_tick_route_work.max(work - self.old_work);
        self.out.max_tick_path_work = self
            .out
            .max_tick_path_work
            .max(m.group_path_work - self.old_path);
        self.old_work = work;
        self.old_path = m.group_path_work;
        self.out.route_work = work - self.start_work[0];
        self.out.member_work = m.group_member_work - self.start_work[1];
        self.out.path_work = m.group_path_work - self.start_work[2];
        self.out.collision_work = m.group_collision_work - self.start_work[3];
        self.out.last_tick = tick;
        self.out.arrived == self.starts.len()
    }

    pub fn finish(mut self, ideal_ticks: u32) -> Metrics {
        self.out.ideal_ticks = ideal_ticks;
        self.out
    }
}

pub(super) fn measure(
    w: &mut GameWorld,
    goals: &[(EntityId, WorldPosition)],
    limit: u32,
    ideal_ticks: u32,
) -> Metrics {
    let mut tracker = Tracker::new(w, &goals.iter().map(|v| v.0).collect::<Vec<_>>());
    tracker.attach(&goals.iter().map(|v| Some(v.1)).collect::<Vec<_>>());
    for tick in 1..=limit {
        w.advance();
        if tracker.observe(w, tick) {
            break;
        }
    }
    tracker.finish(ideal_ticks)
}

pub(super) fn reference_ticks(w: &GameWorld, goals: &[(EntityId, WorldPosition)]) -> u32 {
    let starts = goals
        .iter()
        .map(|&(id, _)| (id, w.unit(id).unwrap().position))
        .collect::<Vec<_>>();
    reference_from(w, &starts, goals)
}

pub(super) fn reference_from(
    w: &GameWorld,
    starts: &[(EntityId, WorldPosition)],
    goals: &[(EntityId, WorldPosition)],
) -> u32 {
    starts
        .iter()
        .zip(goals)
        .map(|(&(id, from), &(goal_id, goal))| {
            assert_eq!(id, goal_id);
            let unit = w.unit(id).unwrap();
            let (speed, denominator) = w.movement_speed(unit.kind, unit.player);
            let a = center(from.tile_floor().x, from.tile_floor().y);
            let b = center(goal.tile_floor().x, goal.tile_floor().y);
            assert!(
                w.body_step_clear(unit.kind, from, a) && w.body_step_clear(unit.kind, b, goal),
                "invalid oracle endpoint connector"
            );
            let cost = shortest(w, unit.kind, from.tile_floor(), goal.tile_floor())
                + distance(from, a) as u64
                + distance(b, goal) as u64;
            (cost * denominator).div_ceil(speed) as u32
        })
        .max()
        .unwrap()
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
