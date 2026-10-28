//! Same 200-army deployment as perf-groups; count only tick 201's movement.
use aoe_simulation::{GameWorld, TickPhase};
use gungraun::prelude::*;
use gungraun::{Callgrind, EntryPoint};
use std::hint::black_box;
#[path = "../../harness/src/perf_scale/groups_workload.rs"]
mod groups_workload;

fn setup() -> GameWorld {
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
    let (mut world, armies) = match groups_workload::build(&Default::default()) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("group benchmark workload failed: {error}");
            std::process::exit(1);
        }
    };
    assert_eq!((armies.spawned, armies.orders_ok), (10_900, 200));
    assert!(armies.refused.is_empty());
    assert_eq!(armies.ids.len(), 200);
    for _ in 0..200 {
        world.advance();
    }
    assert_eq!(world.units().filter(|u| u.moving).count(), 10_900);
    eprintln!(
        "group movement kernel: tick={}, units={}, hash={:x?}",
        world.tick().0,
        world.unit_count(),
        world.canonical_hash()
    );
    world
}

#[library_benchmark(setup = setup, config = LibraryBenchmarkConfig::default().tool(
    Callgrind::with_args(["--instr-atstart=no", "--read-inline-info=yes"]).entry_point(EntryPoint::None)
))]
fn movement(mut world: GameWorld) -> GameWorld {
    gungraun::client_requests::callgrind::start_instrumentation();
    world.advance_with_probe(&mut |_| true, &mut |phase| {
        if phase == TickPhase::Movement {
            gungraun::client_requests::callgrind::stop_instrumentation();
        }
    });
    black_box(world)
}
library_benchmark_group!(name = simulation, benchmarks = [movement]);
main!(library_benchmark_groups = simulation);
