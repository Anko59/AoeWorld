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
        Callgrind::with_args(["--instr-atstart=no", "--separate-threads=yes"])
            .entry_point(EntryPoint::None)
    )
)]
fn late_game(mut world: GameWorld) -> GameWorld {
    // Setup/warm-up is unmeasured. Disable its instrumentation, then count
    // 64 entire authoritative ticks, including all callees and allocations.
    // Shorter windows hid lazily built navigation: a cold build either fell
    // in them or not (one tick: 141M against 168M over eight; eight ticks: a
    // 430M region load doubled the row on #1011, -2.3% over 64).
    gungraun::client_requests::callgrind::start_instrumentation();
    for _ in 0..64 {
        world.advance();
    }
    gungraun::client_requests::callgrind::stop_instrumentation();
    // Return the world: teardown is outside the measured ticks.
    black_box(world)
}

library_benchmark_group!(name = simulation, benchmarks = [late_game]);
main!(library_benchmark_groups = simulation);
