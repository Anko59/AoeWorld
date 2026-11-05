use super::scene::{Cell, OBSTACLE_END, Scene, Shape};
use super::*;
use crate::game_world::TickPhase;
use std::fmt::Write as _;
use std::time::Instant;

const ARRIVED_TILES: i64 = 65;
/// Sixty seconds at 20 Hz with no tile of progress is "permanently stuck".
pub(super) const STUCK_TICKS: u32 = 1_200;
/// Nobody has gained a tile for this long: the march is over.
const STOP_TICKS: u32 = 4_800;

#[derive(Debug, Default)]
pub(super) struct Row {
    pub label: String,
    pub arrived: usize,
    pub total: usize,
    pub idle_short: usize,
    pub stuck: usize,
    pub refused: bool,
    pub t50: u32,
    pub t95: u32,
    pub t100: u32,
    pub ideal: u32,
    pub ticks: u32,
    pub failures: usize,
    pub cross_mean: f64,
    pub tick_us: u64,
    pub tick_p99_us: u64,
    pub movement_us: u64,
    pub generals_us: u64,
    pub route_work: u64,
    pub orders: u64,
    pub hash: String,
    /// Where and how the unfinished units stand, for the `.diag` file.
    pub diagnosis: String,
}

impl Row {
    pub fn tsv(&self) -> String {
        let mut s = String::new();
        let _ = write!(
            s,
            "{}\t{}/{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.1}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.label,
            self.arrived,
            self.total,
            self.idle_short,
            self.stuck,
            self.refused,
            self.t50,
            self.t95,
            self.t100,
            self.ideal,
            self.ticks,
            self.failures,
            self.cross_mean,
            self.tick_us,
            self.tick_p99_us,
            self.movement_us,
            self.generals_us,
            self.route_work,
            self.orders,
            self.hash,
        );
        s
    }
}

pub(super) fn run_cell(cell: Cell, wall_limit_seconds: u64) -> Row {
    let mut scene = Scene::new(cell);
    // The General's first census adopts the block before the march starts.
    let refused = if scene.generals.is_empty() {
        scene.order().is_err()
    } else {
        for _ in 0..40 {
            scene.fixture.world.advance();
        }
        scene.order().is_err()
    };
    run_scene(cell, scene, refused, wall_limit_seconds)
}

pub(super) fn run_scene(cell: Cell, mut scene: Scene, refused: bool, wall_seconds: u64) -> Row {
    let w = &mut scene.fixture.world;
    let slowest = if cell.kind == UnitKind::General {
        UnitKind::SiegeRam
    } else {
        cell.kind
    };
    let (speed, denominator) = w.movement_speed(slowest, PlayerId(0));
    let ideal = ((280 * 1024_u64 * denominator) / speed) as u32;
    let body = i64::from(slowest.collision_radius()) * 2 + 128;
    let file = (cell.count as u64 * body as u64 * denominator / speed) as u32;
    let limit = (ideal * 3 + file * 2).clamp(6_000, 60_000);
    let count = scene.soldiers.len();
    let goal = scene.goal;
    let (mut tiles, mut anchors, mut moved_at) = (
        vec![TileCoord::new(0, 0); count],
        vec![WorldPosition::new(0, 0); count],
        vec![0_u32; count],
    );
    let mut arrived_at: Vec<Option<u32>> = vec![None; count];
    for (i, id) in scene.soldiers.iter().enumerate() {
        let p = w.unit(*id).unwrap().position;
        tiles[i] = p.tile_floor();
        anchors[i] = p;
    }
    let mut row = Row {
        label: format!(
            "{:?}\t{:?}\t{}\t{}",
            cell.shape,
            cell.kind,
            cell.count,
            if cell.general { "general" } else { "plain" }
        ),
        total: count,
        refused,
        ideal,
        ..Row::default()
    };
    let started = Instant::now();
    let metrics = w.route_scheduler_stats().metrics;
    let work0 = metrics.issue_work + metrics.fair_work + metrics.lookahead_work;
    let mut costs = Vec::with_capacity(limit as usize);
    let (mut cross, mut movement, mut generals) = (0_u64, 0_u64, 0_u64);
    for tick in 1..=limit {
        let start = Instant::now();
        let mut phase_at = start;
        w.advance_with_probe(&mut |_| true, &mut |phase| {
            let now = Instant::now();
            let spent = now.duration_since(phase_at).as_micros() as u64;
            match phase {
                TickPhase::Movement => movement += spent,
                TickPhase::StewardEconomy => generals += spent,
                _ => {}
            }
            phase_at = now;
        });
        costs.push(start.elapsed().as_micros() as u64);
        row.orders += w.general_work().orders as u64;
        let mut pending = 0;
        for (i, id) in scene.soldiers.iter().enumerate() {
            let Some(u) = w.unit(*id) else { continue };
            let t = u.position.tile_floor();
            if t != tiles[i] {
                tiles[i] = t;
                cross += 1;
            }
            let (dx, dy) = (
                i64::from(u.position.x) - i64::from(anchors[i].x),
                i64::from(u.position.y) - i64::from(anchors[i].y),
            );
            if dx * dx + dy * dy >= 1024 * 1024 {
                anchors[i] = u.position;
                moved_at[i] = tick;
            }
            let near = {
                let (gx, gy) = (
                    i64::from(u.position.x) - i64::from(goal.x),
                    i64::from(u.position.y) - i64::from(goal.y),
                );
                gx * gx + gy * gy <= (ARRIVED_TILES * 1024).pow(2)
                    && (cell.shape == Shape::Bank || t.x > OBSTACLE_END)
            };
            let done = near && !u.moving && !u.planning;
            if done && arrived_at[i].is_none() {
                arrived_at[i] = Some(tick);
            } else if !done {
                arrived_at[i] = None;
                pending += 1;
            }
        }
        row.ticks = tick;
        if pending == 0 {
            break;
        }
        // Recovery may retry for several minutes before it gives up.
        if tick - moved_at.iter().copied().max().unwrap_or(0) >= STOP_TICKS {
            break;
        }
        if tick % 256 == 0 && started.elapsed().as_secs() >= wall_seconds {
            row.failures = usize::MAX;
            break;
        }
    }
    let mut times: Vec<_> = arrived_at.iter().flatten().copied().collect();
    times.sort_unstable();
    row.arrived = times.len();
    let at = |share: usize| {
        let need = (count * share).div_ceil(100);
        if need > 0 && times.len() >= need {
            times[need - 1]
        } else {
            0
        }
    };
    (row.t50, row.t95, row.t100) = (at(50), at(95), at(100));
    for (i, id) in scene.soldiers.iter().enumerate() {
        if arrived_at[i].is_some() {
            continue;
        }
        let Some(u) = w.unit(*id) else { continue };
        if !u.moving && !u.planning {
            row.idle_short += 1;
        } else if row.ticks - moved_at[i] >= STUCK_TICKS {
            row.stuck += 1;
        }
        if w.movement_failure(*id).is_some() && row.failures != usize::MAX {
            row.failures += 1;
        }
    }
    row.diagnosis = diagnose(w, &scene.soldiers, &arrived_at, &moved_at, row.ticks, goal);
    costs.sort_unstable();
    let ticks = u64::from(row.ticks).max(1);
    row.tick_us = costs.iter().sum::<u64>() / ticks;
    row.tick_p99_us = costs[(costs.len() * 99 / 100).min(costs.len() - 1)];
    row.movement_us = movement / ticks;
    row.generals_us = generals / ticks;
    row.cross_mean = cross as f64 / ticks as f64;
    let metrics = w.route_scheduler_stats().metrics;
    row.route_work =
        (metrics.issue_work + metrics.fair_work + metrics.lookahead_work - work0) / ticks;
    row.hash = format!("{:x?}", w.canonical_hash())
        .chars()
        .filter(char::is_ascii_hexdigit)
        .take(10)
        .collect();
    row
}

fn diagnose(
    w: &GameWorld,
    soldiers: &[EntityId],
    arrived_at: &[Option<u32>],
    moved_at: &[u32],
    ticks: u32,
    goal: WorldPosition,
) -> String {
    let mut bins = std::collections::BTreeMap::<String, u32>::new();
    let mut samples = String::new();
    for (i, id) in soldiers.iter().enumerate() {
        if arrived_at[i].is_some() {
            continue;
        }
        let Some(u) = w.unit(*id) else { continue };
        let t = u.position.tile_floor();
        let place = match t.x {
            ..=119 => "before",
            120..=170 => "obstacle",
            _ if (i64::from(t.x) - i64::from(goal.tile_floor().x)).abs() <= 65 => "at-goal",
            _ => "between",
        };
        let key = format!(
            "{place} moving={} planning={} grouped={} commanded={} detached={} still_ge_stuck={}",
            u.moving,
            u.planning,
            w.formation(*id).is_some(),
            w.general_controller(*id).is_some(),
            w.is_detached_member(w.lookup[id]),
            ticks - moved_at[i] >= STUCK_TICKS,
        );
        *bins.entry(key).or_default() += 1;
        if samples.len() < 4000 {
            let _ = writeln!(
                samples,
                "  {:?} {:?} at=({},{}) order={:?} route={:?} obstruction={:?} still={} slot={:?} anchor={:?} error={:?} {}",
                u.id,
                u.task,
                u.position.x,
                u.position.y,
                w.units[w.lookup[id]].order,
                w.units[w.lookup[id]]
                    .route
                    .iter()
                    .take(4)
                    .collect::<Vec<_>>(),
                w.units[w.lookup[id]].obstruction,
                ticks - moved_at[i],
                w.formation_destination_slot(*id).map(|p| p.tile_floor()),
                w.formation_anchor(*id).map(|p| p.tile_floor()),
                w.movement_failure(*id),
                if w.formation(*id).is_some() {
                    w.group_density_probe(*id)
                        .chars()
                        .take(320)
                        .collect::<String>()
                } else {
                    String::new()
                },
            );
        }
    }
    let mut out = String::new();
    for (key, count) in bins {
        let _ = writeln!(out, "{count:5} {key}");
    }
    out + &samples
}
