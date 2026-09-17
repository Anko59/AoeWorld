use aoe_core::Region;
use aoe_scenario::SMOKE;
use aoe_simulation::World;
use gungraun::Dhat;
use gungraun::prelude::*;
use std::hint::black_box;

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

library_benchmark_group!(name = simulation, benchmarks = [advance, viewport_query]);

main!(
    config = LibraryBenchmarkConfig::default().tool(Dhat::default()),
    library_benchmark_groups = simulation
);
