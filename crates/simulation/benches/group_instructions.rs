//! Same 200-army deployment as perf-groups. `movement` counts only tick 201's
//! movement; `stages_steady` and `stages_start` count whole ticks with the
//! cache simulation on, and `make perf-groups-instructions` splits them per
//! stage (`aoe-harness perf-stage-counters`).
use aoe_simulation::{GameWorld, TickPhase};
use gungraun::prelude::*;
use gungraun::{Callgrind, EntryPoint};
use std::hint::black_box;
#[path = "../../harness/src/perf_scale/groups_workload.rs"]
mod groups_workload;

fn deployed() -> GameWorld {
    static POOL: std::sync::Once = std::sync::Once::new();
    POOL.call_once(|| {
        if let Err(error) = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .use_current_thread()
            .build_global()
        {
            eprintln!("group benchmark worker setup failed: {error}");
            std::process::exit(1);
        }
    });
    let (world, armies) = match groups_workload::build(&Default::default()) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("group benchmark workload failed: {error}");
            std::process::exit(1);
        }
    };
    assert_eq!(
        (armies.spawned, armies.orders_ok),
        (groups_workload::DEFAULT_MOVERS, 200)
    );
    assert!(armies.refused.is_empty());
    assert_eq!(armies.ids.len(), 200);
    world
}

fn setup() -> GameWorld {
    let mut world = deployed();
    for _ in 0..groups_workload::STEADY_WARMUP_TICKS {
        world.advance();
    }
    assert_eq!(
        world.units().filter(|u| u.moving).count(),
        groups_workload::DEFAULT_MOVERS as usize
    );
    eprintln!(
        "group movement kernel: tick={}, units={}, hash={:x?}",
        world.tick().0,
        world.unit_count(),
        world.canonical_hash()
    );
    world
}

/// The deployment with its orders just issued: the first tick plans and
/// searches, which the steady tick does not.
fn setup_start() -> GameWorld {
    deployed()
}

fn counted() -> LibraryBenchmarkConfig {
    let mut config = LibraryBenchmarkConfig::default();
    config.tool(
        Callgrind::with_args(["--instr-atstart=no", "--read-inline-info=yes"])
            .entry_point(EntryPoint::None),
    );
    config
}

#[library_benchmark(setup = setup, config = counted())]
fn movement(mut world: GameWorld) -> GameWorld {
    gungraun::client_requests::callgrind::start_instrumentation();
    world.advance_with_probe(&mut |_| true, &mut |phase| {
        if phase == TickPhase::Movement {
            gungraun::client_requests::callgrind::stop_instrumentation();
        }
    });
    black_box(world)
}

// One whole steady tick (201), every phase and the changed-unit collation.
#[library_benchmark(setup = setup, config = counted())]
fn stages_steady(mut world: GameWorld) -> GameWorld {
    gungraun::client_requests::callgrind::start_instrumentation();
    world.advance();
    gungraun::client_requests::callgrind::stop_instrumentation();
    black_box(world)
}

// One whole first tick (1), after the orders.
#[library_benchmark(setup = setup_start, config = counted())]
fn stages_start(mut world: GameWorld) -> GameWorld {
    gungraun::client_requests::callgrind::start_instrumentation();
    world.advance();
    gungraun::client_requests::callgrind::stop_instrumentation();
    black_box(world)
}
library_benchmark_group!(
    name = simulation,
    benchmarks = [movement, stages_steady, stages_start]
);
main!(library_benchmark_groups = simulation);
