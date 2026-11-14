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
    // eight entire authoritative ticks, including all callees and allocations.
    // One tick hid lazily built navigation: a cold portal build either fell in
    // it or not, and dev's tick at 385 cost 141M against 168M over the eight.
    gungraun::client_requests::callgrind::start_instrumentation();
    for _ in 0..8 {
        world.advance();
    }
    gungraun::client_requests::callgrind::stop_instrumentation();
    // Return the world: teardown is outside the measured ticks.
    black_box(world)
}

library_benchmark_group!(name = simulation, benchmarks = [late_game]);
main!(library_benchmark_groups = simulation);
