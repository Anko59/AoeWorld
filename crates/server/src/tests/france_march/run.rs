//! One scoreboard row: spawn on real ground, order the march, measure.
use super::*;
use aoe_core::{EntityId, Formation, PlayerId, WorldPosition};
use aoe_simulation::UnitKind;
use std::fmt::Write as _;
use std::io::Write as _;
use std::time::Instant;

/// A soldier stopped this near (straight and walking) has arrived.
const ARRIVED_TILES: i64 = 40;
const ARRIVED_WALK: u32 = 64 * 1024;
const STUCK_TICKS: u32 = 1_200;
/// No member got any walking distance closer for this long: the march is over.
const STOP_TICKS: u32 = 2_400;
const WALL_SECONDS: u64 = 300;

type Deck = ((i32, i32), (i32, i32));

#[derive(Clone, Copy, Debug)]
pub(super) struct Pair {
    pub name: &'static str,
    pub start: (i32, i32),
    pub goal: (i32, i32),
    pub bridges: &'static [Deck],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Army {
    Paladins,
    Champions,
    Mixed,
    Rams,
}

impl Army {
    fn kind(self, n: usize) -> UnitKind {
        match self {
            Self::Paladins => UnitKind::Paladin,
            Self::Champions => UnitKind::Champion,
            Self::Rams => UnitKind::SiegeRam,
            Self::Mixed => match n {
                0..40 => UnitKind::Champion,
                40..70 => UnitKind::Archer,
                70..90 => UnitKind::Paladin,
                _ => UnitKind::SiegeRam,
            },
        }
    }

    fn slowest(self) -> UnitKind {
        match self {
            Self::Paladins => UnitKind::Paladin,
            Self::Champions => UnitKind::Champion,
            Self::Mixed | Self::Rams => UnitKind::SiegeRam,
        }
    }

    fn lattice(self) -> i32 {
        match self {
            Self::Paladins | Self::Mixed => 2,
            Self::Champions => 1,
            Self::Rams => 3,
        }
    }
}

fn centre(tile: TileCoord) -> WorldPosition {
    WorldPosition::from_tile_center(tile).unwrap()
}

/// The army on tiles the goal can be walked to from, nearest the start first.
pub(super) fn spawn(
    world: &mut GameWorld,
    field: &WalkingField,
    start: TileCoord,
    army: Army,
    general: bool,
) -> (Vec<EntityId>, Vec<EntityId>) {
    let gap = army.lattice();
    let mut tiles = Vec::new();
    for y in -48..=48 {
        for x in -48..=48 {
            let t = TileCoord::new(start.x + x * gap, start.y + y * gap);
            if (x, y) != (0, 0) && field.cost(t).is_some() {
                tiles.push((x * x + y * y, t.y, t.x));
            }
        }
    }
    tiles.sort_unstable();
    let mut soldiers = Vec::new();
    for (_, y, x) in tiles {
        if soldiers.len() == 100 {
            break;
        }
        let kind = army.kind(soldiers.len());
        if let Ok(id) = world.spawn_kind(PlayerId(0), kind, centre(TileCoord::new(x, y))) {
            soldiers.push(id);
        }
    }
    assert_eq!(soldiers.len(), 100, "the army must fit on the start ground");
    // A General leads at most ten squads of five rams: fifty, so a ram army has two.
    let generals = match (general, army) {
        (false, _) => &[][..],
        (true, Army::Rams) => &[(0, 0), (1, 1)][..],
        (true, _) => &[(0, 0)][..],
    };
    let commanders = generals
        .iter()
        .map(|&(x, y)| {
            let at = TileCoord::new(start.x + x, start.y + y);
            world
                .spawn_kind(PlayerId(0), UnitKind::General, centre(at))
                .unwrap()
        })
        .collect();
    (soldiers, commanders)
}

pub(super) struct Setup {
    pub world: GameWorld,
    pub field: WalkingField,
}

pub(super) fn setup(pair: Pair) -> Setup {
    let mut world = france();
    for &(from, to) in pair.bridges {
        world.place_finished_bridge(span(from, to)).unwrap();
    }
    let (start, goal) = (
        TileCoord::new(pair.start.0, pair.start.1),
        TileCoord::new(pair.goal.0, pair.goal.1),
    );
    let field = field(&world, goal, start, 500);
    assert!(
        field.cost(start).is_some(),
        "{} has no walking route",
        pair.name
    );
    Setup { world, field }
}

/// Marches, records the row, and returns the final hash for replay checks.
pub(super) fn march(pair: Pair, army: Army, general: bool, mut setup: Setup) -> [u8; 32] {
    let world = &mut setup.world;
    let field = &setup.field;
    let start = TileCoord::new(pair.start.0, pair.start.1);
    let goal_tile = TileCoord::new(pair.goal.0, pair.goal.1);
    let goal = centre(goal_tile);
    let truth = u64::from(field.cost(start).unwrap());
    let (soldiers, commanders) = spawn(world, field, start, army, general);
    let mut bodies = soldiers.clone();
    let order = |world: &mut GameWorld| {
        if commanders.is_empty() {
            return world
                .issue_formation_move(PlayerId(0), &soldiers, Formation::Line, goal, false, false)
                .map_err(|e| format!("{e:?}"));
        }
        for _ in 0..40 {
            world.advance();
        }
        for &g in &commanders {
            world.issue_move(g, goal).map_err(|e| format!("{e:?}"))?;
        }
        Ok(())
    };
    let refused = order(world).err();
    bodies.extend(&commanders);
    let commander = commanders.first().copied();
    let (speed, denominator) = world.movement_speed(army.slowest(), PlayerId(0));
    let ideal = (truth * denominator / speed).max(1) as u32;
    // The whole file must pass a chokepoint too; a limit, not the yardstick.
    let limit = (ideal * 4).max(6_000);
    let n = bodies.len();
    let mut best_left = vec![u32::MAX; n];
    let mut tile_at = vec![TileCoord::new(-1, -1); n];
    let mut moved_at = vec![0_u32; n];
    let mut arrived_at: Vec<Option<u32>> = vec![None; n];
    let (mut progress_at, mut ticks) = (0_u32, 0_u32);
    let (mut max_step_us, mut costs) = (0_u64, Vec::with_capacity(limit as usize));
    let metrics = world.route_scheduler_stats().metrics;
    let work0 = metrics.issue_work + metrics.fair_work + metrics.lookahead_work;
    let began = Instant::now();
    let mut trace = String::new();
    for tick in 1..=limit {
        let step = Instant::now();
        world.advance();
        let spent = step.elapsed().as_micros() as u64;
        costs.push(spent);
        max_step_us = max_step_us.max(spent);
        ticks = tick;
        let mut pending = 0;
        for (i, &id) in bodies.iter().enumerate() {
            let Some(u) = world.unit(id) else { continue };
            let t = u.position.tile_floor();
            if t != tile_at[i] {
                tile_at[i] = t;
                moved_at[i] = tick;
            }
            let left = field.cost(t).unwrap_or(u32::MAX);
            if left < best_left[i] {
                if best_left[i] != u32::MAX && best_left[i] - left >= 1024 {
                    progress_at = tick;
                }
                best_left[i] = left;
            }
            let (dx, dy) = (
                i64::from(u.position.x - goal.x),
                i64::from(u.position.y - goal.y),
            );
            let near = dx * dx + dy * dy <= (ARRIVED_TILES * 1024).pow(2) && left <= ARRIVED_WALK;
            if near && !u.moving && !u.planning {
                arrived_at[i].get_or_insert(tick);
            } else {
                arrived_at[i] = None;
                pending += 1;
            }
        }
        if tick % 500 == 1 {
            let _ = writeln!(
                trace,
                "{tick} {}",
                snapshot(world, field, &bodies, commander)
            );
        }
        if pending == 0
            || tick - progress_at >= STOP_TICKS
            || (tick % 256 == 0 && began.elapsed().as_secs() >= WALL_SECONDS)
        {
            break;
        }
    }
    let mut times: Vec<_> = arrived_at.iter().flatten().copied().collect();
    times.sort_unstable();
    let at = |share: usize| {
        let need = (n * share).div_ceil(100);
        if need > 0 && times.len() >= need {
            times[need - 1]
        } else {
            0
        }
    };
    let (mut idle, mut stuck) = (0, 0);
    for (i, &id) in bodies.iter().enumerate() {
        let Some(u) = world.unit(id) else { continue };
        if arrived_at[i].is_some() {
            continue;
        }
        if !u.moving && !u.planning {
            idle += 1;
        } else if ticks - moved_at[i] >= STUCK_TICKS {
            stuck += 1;
        }
    }
    costs.sort_unstable();
    let metrics = world.route_scheduler_stats().metrics;
    let work = metrics.issue_work + metrics.fair_work + metrics.lookahead_work - work0;
    let last = at(100);
    let ratio = if last > 0 {
        f64::from(last) / f64::from(ideal)
    } else {
        0.0
    };
    let label = format!(
        "{}\t{army:?}\t{}",
        pair.name,
        if general { "general" } else { "plain" }
    );
    let line = format!(
        "{label}\t{}\t{:.0}\t{}/{n}\t{idle}\t{stuck}\t{}\t{}\t{last}\t{ideal}\t{ratio:.2}\t{ticks}\t{}\t{}\t{}\t{}\t{}",
        truth / 1024,
        (f64::from(pair.goal.0 - pair.start.0).powi(2)
            + f64::from(pair.goal.1 - pair.start.1).powi(2))
        .sqrt(),
        times.len(),
        at(50),
        at(95),
        costs.iter().sum::<u64>() / u64::from(ticks).max(1),
        costs[(costs.len() * 99 / 100).min(costs.len() - 1)],
        work / u64::from(ticks).max(1),
        refused.as_deref().unwrap_or("-"),
        snapshot(world, field, &bodies, commander),
    );
    let _ = writeln!(trace, "end {ticks}");
    write_out(&label, &line, &trace, world, &bodies, &arrived_at);
    world.canonical_hash()
}

/// General and army, in walking tiles from the goal.
fn snapshot(
    world: &GameWorld,
    field: &WalkingField,
    bodies: &[EntityId],
    commander: Option<EntityId>,
) -> String {
    let left = |id| {
        world
            .unit(id)
            .and_then(|u| field.cost(u.position.tile_floor()))
            .map_or(-1, |c| i64::from(c / 1024))
    };
    let mut lefts: Vec<_> = bodies.iter().map(|&id| left(id)).collect();
    lefts.sort_unstable();
    let g = commander
        .and_then(|id| world.unit(id).map(|u| (u.position.tile_floor(), left(id))))
        .map(|(t, l)| format!("G({},{}) left {l} ", t.x, t.y))
        .unwrap_or_default();
    let moving = bodies
        .iter()
        .filter(|&&id| world.unit(id).is_some_and(|u| u.moving || u.planning))
        .count();
    format!(
        "{g}left min {} med {} max {} moving {moving}",
        lefts[0],
        lefts[lefts.len() / 2],
        lefts[lefts.len() - 1]
    )
}

fn write_out(
    label: &str,
    line: &str,
    trace: &str,
    world: &GameWorld,
    bodies: &[EntityId],
    arrived_at: &[Option<u32>],
) {
    let Some(name) = super::label() else { return };
    let dir = output();
    let append = |path: String, text: String| {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(path))
            .unwrap();
        file.write_all(text.as_bytes()).unwrap();
    };
    append(format!("{name}.tsv"), format!("{line}\n"));
    let mut diag = format!("== {label}\n{trace}");
    for (i, &id) in bodies.iter().enumerate() {
        if arrived_at[i].is_some() {
            continue;
        }
        if let Some(u) = world.unit(id) {
            let t = u.position.tile_floor();
            let _ = writeln!(
                diag,
                "  {:?} {:?} ({},{}) moving={} planning={} task={:?} formation={:?} ctrl={:?} err={:?}",
                u.id,
                u.kind,
                t.x,
                t.y,
                u.moving,
                u.planning,
                u.task,
                world.formation(id),
                world.general_controller(id),
                world.movement_failure(id),
            );
            let _ = writeln!(diag, "    {}", world.formation_member_status(id));
        }
    }
    append(format!("{name}.diag"), diag);
}
