//! `perf-groups`: about 200 armies (11,000 military movers) cross one another's
//! path on open ground. It reports the movement phase and the
//! movement spans named in docs/movement/group-movement-rewrite.md section 5.2.
//! Synthetic: it does not qualify a finished RTS or dedicated hardware.
use super::movement::MovementProbe;
use super::stats::{Series, memory, round};
use super::world::BoxError;
use super::{PHASES, git};
use aoe_core::{Formation, PlayerId, Seed, WorldConfig, WorldPosition};
use aoe_simulation::{GameWorld, TickPhase, UnitKind, WorldLimits};
use clap::Args;
use serde_json::json;
use std::{fs, path::PathBuf, sync::Arc, time::Instant};

#[derive(Args, Debug, Clone)]
pub struct Options {
    #[arg(long, default_value_t = 200)]
    armies: u32,
    /// Army sizes cycle through this range, so the mean is the midpoint.
    #[arg(long, default_value_t = 30)]
    min_size: u32,
    #[arg(long, default_value_t = 79)]
    max_size: u32,
    /// Army blocks sit on a grid of this many tiles per cell.
    #[arg(long, default_value_t = 24)]
    cell_tiles: i32,
    #[arg(long, default_value_t = 1_000)]
    side_tiles: u32,
    #[arg(long, default_value_t = 100)]
    players: u16,
    /// Ticks before measuring (the armies assemble and set off).
    #[arg(long, default_value_t = 100)]
    warmup_ticks: u32,
    /// Ticks per window: one without spans, one with them.
    #[arg(long, default_value_t = 300)]
    ticks: u32,
    #[arg(long, default_value_t = 1)]
    seed: u64,
    #[arg(long)]
    fog_off: bool,
    /// Attack-move instead of move: crossing armies of different players
    /// meet, so the run also measures the attack-move look and the fights.
    #[arg(long)]
    attack_move: bool,
    #[arg(long, default_value = "reports/perf/perf-groups.json")]
    out: PathBuf,
}

fn at(x: i32, y: i32) -> WorldPosition {
    // The centre of tile (x, y): 1,024 subunits per tile.
    WorldPosition::new(x * 1024 + 512, y * 1024 + 512)
}

/// Each army marches to the mirror cell inside its block of four columns, so
/// the pairs 0-3 and 1-2 pass through each other; every third army also
/// crosses to the other row of its pair of rows.
fn partner(index: u32, columns: u32) -> u32 {
    let (cx, cy) = (index % columns, index / columns);
    let py = if index.is_multiple_of(3) { cy ^ 1 } else { cy };
    py * columns + (cx ^ 3)
}

struct Armies {
    ids: Vec<Vec<aoe_core::EntityId>>,
    orders_ok: u32,
    spawned: u32,
    refused: std::collections::BTreeMap<String, u32>,
}

/// Trees and water close some tiles: try the nearest others.
fn nearby(x: i32, y: i32) -> impl Iterator<Item = (i32, i32)> {
    (0..64).map(move |k| (x + k % 8 * (1 - k / 8 % 2 * 2), y + k / 8))
}

fn deploy(world: &mut GameWorld, o: &Options) -> Result<Armies, BoxError> {
    let columns = 20_u32;
    let rows = o.armies.div_ceil(columns);
    let (cell, side) = (o.cell_tiles, o.side_tiles as i32);
    let origin = (
        (side - columns as i32 * cell) / 2,
        (side - rows as i32 * cell) / 2,
    );
    let corner = |i: u32| {
        (
            origin.0 + (i % columns) as i32 * cell,
            origin.1 + (i / columns) as i32 * cell,
        )
    };
    let mut out = Armies {
        ids: Vec::new(),
        orders_ok: 0,
        spawned: 0,
        refused: Default::default(),
    };
    for i in 0..o.armies {
        let player = PlayerId((i % u32::from(o.players)) as u16);
        let size = o.min_size + (i * 17) % (o.max_size - o.min_size + 1);
        let (x0, y0) = corner(i);
        let mut ids = Vec::new();
        for n in 0..size as i32 {
            let wanted = (x0 + n % 10, y0 + n / 10);
            let spawned = nearby(wanted.0, wanted.1)
                .find_map(|(x, y)| world.spawn_kind(player, UnitKind::Militia, at(x, y)).ok());
            ids.extend(spawned);
        }
        out.spawned += ids.len() as u32;
        out.ids.push(ids);
    }
    for i in 0..o.armies {
        let ids = &out.ids[i as usize];
        if ids.is_empty() {
            continue;
        }
        let (x1, y1) = corner(partner(i, columns).min(o.armies - 1));
        let player = PlayerId((i % u32::from(o.players)) as u16);
        let mut last = None;
        let ok = nearby(x1 + 4, y1 + 4).any(|(x, y)| {
            let sent = world.issue_formation_move(
                player,
                ids,
                Formation::Line,
                at(x, y),
                false,
                o.attack_move,
            );
            last = sent.err();
            last.is_none()
        });
        out.orders_ok += u32::from(ok);
        if let Some(error) = last.filter(|_| !ok) {
            *out.refused.entry(format!("{error:?}")).or_default() += 1;
        }
    }
    Ok(out)
}

fn window(world: &mut GameWorld, ticks: u32) -> (Series, Series, Series) {
    let (mut tick, mut movement, mut rest) =
        (Series::default(), Series::default(), Series::default());
    for _ in 0..ticks {
        let begin = Instant::now();
        let mut last = begin;
        let mut spent = [0u128; PHASES.len()];
        world.advance_with_probe(&mut |_| true, &mut |phase: TickPhase| {
            let now = Instant::now();
            spent[phase as usize] += (now - last).as_nanos();
            last = now;
        });
        tick.push_ms(begin.elapsed().as_nanos());
        movement.push_ms(spent[0]);
        rest.push_ms(spent[1..].iter().sum());
    }
    (tick, movement, rest)
}

pub fn run(o: Options) -> Result<(), BoxError> {
    let started = Instant::now();
    let mut world = GameWorld::new(WorldConfig::new(
        o.side_tiles as i32,
        o.side_tiles as i32,
        Seed(o.seed),
    )?)?;
    world.set_limits(WorldLimits {
        players: 4_096,
        buildings: 1 << 20,
        gatherers: 1 << 20,
        builders: 1 << 20,
        hunters: 1 << 20,
        animals: 1 << 20,
        projectiles: 1 << 20,
        population: 2_000,
    });
    world.set_fog_disabled(o.fog_off);
    let armies = deploy(&mut world, &o)?;
    let build_s = started.elapsed().as_secs_f64();
    eprintln!(
        "perf-groups: {} units in {} armies, {} orders accepted",
        armies.spawned,
        armies.ids.len(),
        armies.orders_ok
    );
    window(&mut world, o.warmup_ticks);
    let plain = window(&mut world, o.ticks);
    let census = |world: &GameWorld| {
        let mut failures = std::collections::BTreeMap::<String, u32>::new();
        for ids in &armies.ids {
            if let Some(error) = ids.iter().find_map(|&id| world.movement_failure(id)) {
                *failures.entry(format!("{error:?}")).or_default() += 1;
            }
        }
        json!({
            "tick": world.tick().0,
            "units": world.unit_count(),
            "active_movers": world.active_mover_count(),
            "moving": world.units().filter(|u| u.moving).count(),
            "planning": world.units().filter(|u| u.planning).count(),
            "armies_with_a_movement_failure": failures,
        })
    };
    let census_plain = census(&world);
    let probe = Arc::new(MovementProbe::default());
    world.set_planning_probe(probe.clone());
    let profiled = window(&mut world, o.ticks);
    let spans = probe.drain();
    let summarize = |w: &(Series, Series, Series)| json!({ "tick": w.0.summary(), "movement": w.1.summary(), "other_phases": w.2.summary() });
    let report = json!({
        "version": 1,
        "revision": git(&["rev-parse", "HEAD"]),
        "dirty": !git(&["status", "--porcelain"]).is_empty(),
        "workload": {
            "armies": armies.ids.len(), "units": armies.spawned, "orders_accepted": armies.orders_ok, "orders_refused": armies.refused,
            "side_tiles": o.side_tiles, "cell_tiles": o.cell_tiles, "players": o.players,
            "fog": !o.fog_off, "warmup_ticks": o.warmup_ticks, "ticks_per_window": o.ticks,
            "map": "uniform open ground (no trees, water, buildings or route planning)",
            "orders": "Line formation moves to the mirror cell in a block of four grid columns; every third army also changes row",
            "attack_move": o.attack_move,
        },
        "build_seconds": round(build_s),
        "census_end_of_plain_window": census_plain,
        "census_end": census(&world),
        "plain_window": summarize(&plain),
        "profiled_window": summarize(&profiled),
        "movement_spans_profiled_window": spans,
        "memory": memory(),
        "note": "spans are inclusive and overlap; the profiled window adds clock and atomic cost",
    });
    if let Some(parent) = o.out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&o.out, serde_json::to_string(&report)? + "\n")?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    println!("report: {}", o.out.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn a_tiny_run_writes_a_report_with_every_movement_span() {
        let directory = tempfile::tempdir().unwrap();
        let out = directory.path().join("groups.json");
        run(Options {
            armies: 6,
            min_size: 5,
            max_size: 9,
            cell_tiles: 24,
            side_tiles: 1_000,
            players: 3,
            warmup_ticks: 5,
            ticks: 10,
            seed: 1,
            fog_off: false,
            attack_move: true,
            out: out.clone(),
        })
        .unwrap();
        let report: Value = serde_json::from_str(&fs::read_to_string(out).unwrap()).unwrap();
        assert_eq!(report["workload"]["armies"], 6);
        assert_eq!(report["workload"]["attack_move"], true);
        assert_eq!(report["plain_window"]["tick"]["samples"], 10);
        for span in [
            "avoidance_query",
            "local_refinement",
            "sidestep",
            "arrival_search",
        ] {
            assert!(
                report["movement_spans_profiled_window"][span]["calls"].is_number(),
                "{span}"
            );
        }
    }
}
