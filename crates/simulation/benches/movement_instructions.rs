//! Focused instruction-only iteration for the `perf-smoke`/`perf-ci` ticks.
use aoe_simulation::GameWorld;
use gungraun::prelude::*;
use gungraun::{Callgrind, EntryPoint};
use std::hint::black_box;
mod support;
use support::{four_ticks, setup_distributed, setup_late_game, setup_smoke};

#[library_benchmark(
    setup = setup_smoke,
    config = LibraryBenchmarkConfig::default().tool(
        Callgrind::with_args(["--instr-atstart=no"]).entry_point(EntryPoint::None)
    )
)]
fn smoke_ticks(world: GameWorld) -> GameWorld {
    four_ticks(world)
}

#[library_benchmark(
    setup = setup_distributed,
    config = LibraryBenchmarkConfig::default().tool(
        Callgrind::with_args(["--instr-atstart=no"]).entry_point(EntryPoint::None)
    )
)]
fn distributed_ticks(world: GameWorld) -> GameWorld {
    four_ticks(world)
}

#[library_benchmark(
    setup = setup_late_game,
    config = LibraryBenchmarkConfig::default().tool(
        Callgrind::with_args(["--instr-atstart=no"]).entry_point(EntryPoint::None)
    )
)]
fn late_game(mut world: GameWorld) -> GameWorld {
    gungraun::client_requests::callgrind::start_instrumentation();
    world.advance();
    gungraun::client_requests::callgrind::stop_instrumentation();
    black_box(world)
}

library_benchmark_group!(
    name = movement,
    benchmarks = [smoke_ticks, distributed_ticks, late_game]
);
main!(library_benchmark_groups = movement);
