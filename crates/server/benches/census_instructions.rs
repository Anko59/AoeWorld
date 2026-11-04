//! Compare the actual serial report kernel with 1,000 players and 100k units.
use aoe_server::GameplayService;
use aoe_simulation::GameWorld;
use gungraun::prelude::*;
use gungraun::{Callgrind, EntryPoint};
use std::hint::black_box;

fn setup() -> GameWorld {
    let world = match GameplayService::census_benchmark_world() {
        Ok(world) => world,
        Err(error) => {
            eprintln!("census fixture setup failed: {error}");
            std::process::exit(1);
        }
    };
    // Exclude the one-time initial census. Idle reports must reuse it thereafter.
    let census = world.unit_census();
    assert_eq!(census.len(), 1000);
    assert_eq!(census.values().flatten().sum::<u32>(), 100_000);
    eprintln!(
        "census return type: {}",
        std::any::type_name_of_val(&census)
    );
    black_box(census);
    world
}

fn reports(world: GameWorld, connected: usize) -> GameWorld {
    gungraun::client_requests::callgrind::start_instrumentation();
    for _ in 0..20 {
        assert_eq!(
            GameplayService::benchmark_player_reports(&world, connected),
            connected
        );
    }
    gungraun::client_requests::callgrind::stop_instrumentation();
    black_box(world)
}

#[library_benchmark(setup = setup, config = LibraryBenchmarkConfig::default().tool(
    Callgrind::with_args(["--instr-atstart=no"]).entry_point(EntryPoint::None)
))]
fn one_viewer(world: GameWorld) -> GameWorld {
    reports(world, 1)
}

#[library_benchmark(setup = setup, config = LibraryBenchmarkConfig::default().tool(
    Callgrind::with_args(["--instr-atstart=no"]).entry_point(EntryPoint::None)
))]
fn thousand_viewers(world: GameWorld) -> GameWorld {
    reports(world, 1000)
}

#[library_benchmark(setup = setup, config = LibraryBenchmarkConfig::default().tool(
    Callgrind::with_args(["--instr-atstart=no"]).entry_point(EntryPoint::None)
))]
fn no_viewers(world: GameWorld) -> GameWorld {
    reports(world, 0)
}

library_benchmark_group!(
    name = census,
    benchmarks = [no_viewers, one_viewer, thousand_viewers]
);
main!(library_benchmark_groups = census);
