//! Focused instruction-only iteration.
use aoe_simulation::GameWorld;
use gungraun::prelude::*;
use gungraun::{Callgrind, EntryPoint};
use std::hint::black_box;
mod support;
use support::setup_late_game;

#[library_benchmark(
    setup = setup_late_game,
    config = LibraryBenchmarkConfig::default().tool(
        Callgrind::with_args(["--instr-atstart=no"])
            .entry_point(EntryPoint::None)
    )
)]
fn late_game(mut world: GameWorld) -> GameWorld {
    // Setup/warm-up is unmeasured. Disable its instrumentation, then count
    // the entire authoritative tick, including all callees and allocations.
    gungraun::client_requests::callgrind::start_instrumentation();
    world.advance();
    gungraun::client_requests::callgrind::stop_instrumentation();
    // Return the world: teardown is outside the measured tick.
    black_box(world)
}

library_benchmark_group!(name = simulation, benchmarks = [late_game]);
main!(library_benchmark_groups = simulation);
