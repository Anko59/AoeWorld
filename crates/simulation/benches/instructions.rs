use aoe_core::Region;
use aoe_scenario::SMOKE;
use aoe_simulation::{GameWorld, World};
use gungraun::prelude::*;
use gungraun::{Callgrind, Dhat, EntryPoint};
use std::hint::black_box;

mod support;
use support::{four_ticks, setup_distributed, setup_late_game, setup_smoke};

fn setup() -> World {
    World::new(SMOKE)
}

#[library_benchmark(setup = setup)]
fn advance(mut world: World) -> u64 {
    world.advance();
    black_box(world.tick().0)
}

#[library_benchmark(setup = setup)]
fn viewport_query(world: World) -> usize {
    black_box(
        world
            .query(Region {
                x: 128,
                y: 128,
                width: 256,
                height: 256,
            })
            .len(),
    )
}

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

// The ticks `perf-smoke` and `perf-ci` time for their population workloads,
// where movement, collision and reservation costs show up.
#[library_benchmark(
    setup = setup_smoke,
    config = LibraryBenchmarkConfig::default().tool(
        Callgrind::with_args(["--instr-atstart=no"])
            .entry_point(EntryPoint::None)
    )
)]
fn smoke_ticks(world: GameWorld) -> GameWorld {
    four_ticks(world)
}

#[library_benchmark(
    setup = setup_distributed,
    config = LibraryBenchmarkConfig::default().tool(
        Callgrind::with_args(["--instr-atstart=no"])
            .entry_point(EntryPoint::None)
    )
)]
fn distributed_ticks(world: GameWorld) -> GameWorld {
    four_ticks(world)
}

library_benchmark_group!(
    name = simulation,
    benchmarks = [
        advance,
        viewport_query,
        late_game,
        smoke_ticks,
        distributed_ticks
    ]
);

main!(
    config = LibraryBenchmarkConfig::default().tool(Dhat::default()),
    library_benchmark_groups = simulation
);
