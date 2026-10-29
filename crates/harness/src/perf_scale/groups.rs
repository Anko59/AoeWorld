//! `perf-groups`: about 200 armies (11,000 military movers) cross one another's
//! path on open ground. It reports the movement phase and the
//! movement spans named in docs/movement/group-movement-rewrite.md section 5.2.
//! Synthetic: it does not qualify a finished RTS or dedicated hardware.
use super::movement::MovementProbe;
use super::stats::{Series, memory, round};
use super::world::BoxError;
use super::{PHASES, git};
use aoe_simulation::{GameWorld, TickPhase};
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
    /// Time coarse prepare/propose/apply stages instead of individual queries.
    #[arg(long)]
    coarse_groups: bool,
    #[arg(long, default_value = "reports/perf/perf-groups.json")]
    out: PathBuf,
}

impl Options {
    fn parameters(&self) -> super::groups_workload::Parameters {
        super::groups_workload::Parameters {
            armies: self.armies,
            min_size: self.min_size,
            max_size: self.max_size,
            cell_tiles: self.cell_tiles,
            side_tiles: self.side_tiles,
            players: self.players,
            seed: self.seed,
            fog_off: self.fog_off,
            attack_move: self.attack_move,
        }
    }
}

type Window = (Series, Series, Series, [Series; PHASES.len()]);

fn window(world: &mut GameWorld, ticks: u32) -> Window {
    let (mut tick, mut movement, mut rest) =
        (Series::default(), Series::default(), Series::default());
    let mut phases = std::array::from_fn(|_| Series::default());
    for _ in 0..ticks {
        let begin = Instant::now();
        let mut last = begin;
        let mut spent = [0u128; PHASES.len()];
        world.advance_with_probe(&mut |_| true, &mut |phase: TickPhase| {
            let now = Instant::now();
            spent[phase as usize] += (now - last).as_nanos();
            last = now;
        });
        let end = Instant::now();
        spent[PHASES.len() - 1] += (end - last).as_nanos();
        tick.push_ms((end - begin).as_nanos());
        movement.push_ms(spent[0]);
        rest.push_ms(spent[1..].iter().sum());
        for (series, nanos) in phases.iter_mut().zip(spent) {
            series.push_ms(nanos);
        }
    }
    (tick, movement, rest, phases)
}

pub fn run(o: Options) -> Result<(), BoxError> {
    let started = Instant::now();
    let (mut world, armies) = super::groups_workload::build(&o.parameters())?;
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
    let probe = Arc::new(if o.coarse_groups {
        MovementProbe::coarse()
    } else {
        MovementProbe::default()
    });
    world.set_planning_probe(probe.clone());
    let profiled = window(&mut world, o.ticks);
    let spans = probe.drain();
    let summarize = |w: &Window| {
        json!({ "tick": w.0.summary(), "movement": w.1.summary(),
        "other_phases": w.2.summary(), "phases": PHASES.iter().zip(&w.3)
            .map(|(&name, series)| (name.to_owned(), series.summary()))
            .collect::<serde_json::Map<_, _>>() })
    };
    let report = json!({
        "version": 1,
        "execution": { "workers": world.movement_worker_count(),
            "available_parallelism": std::thread::available_parallelism().map(|n| n.get()).ok(),
            "slot_cpus": std::env::var("AOE_SLOT_CPUS").ok() },
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
        "coarse_groups": o.coarse_groups,
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
    fn report_omits_spans_that_were_not_selected_for_measurement() {
        use aoe_simulation::PlanningProbe;
        for (probe, enabled, disabled) in [
            (MovementProbe::default(), "avoidance_query", "group_apply"),
            (MovementProbe::coarse(), "group_apply", "avoidance_query"),
        ] {
            drop(probe.movement_span(enabled).unwrap());
            assert!(probe.movement_span(disabled).is_none());
            let report = probe.drain();
            assert_eq!(report[enabled]["calls"], 1);
            assert!(report.get(disabled).is_none());
        }
    }

    #[test]
    fn instruction_kernel_uses_the_cli_default_workload() {
        #[derive(clap::Parser)]
        struct Cli {
            #[command(flatten)]
            options: Options,
        }
        let cli = <Cli as clap::Parser>::parse_from(["perf-groups"]);
        assert_eq!(
            cli.options.parameters(),
            super::super::groups_workload::Parameters::default()
        );
    }

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
            coarse_groups: false,
            out: out.clone(),
        })
        .unwrap();
        let report: Value = serde_json::from_str(&fs::read_to_string(out).unwrap()).unwrap();
        assert_eq!(report["workload"]["armies"], 6);
        assert_eq!(report["workload"]["attack_move"], true);
        assert_eq!(report["plain_window"]["tick"]["samples"], 10);
        for phase in PHASES {
            assert_eq!(report["plain_window"]["phases"][phase]["samples"], 10);
        }
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
