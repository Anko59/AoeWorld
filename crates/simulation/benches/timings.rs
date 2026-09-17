use aoe_core::Region;
use aoe_scenario::SMOKE;
use aoe_simulation::World;
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

fn benchmarks(c: &mut Criterion) {
    let mut world = World::new(SMOKE);
    c.bench_function("simulation_tick_smoke_8k", |bench| {
        bench.iter(|| {
            world.advance();
            black_box(world.tick());
        });
    });
    let world = World::new(SMOKE);
    let region = Region {
        x: 128,
        y: 128,
        width: 256,
        height: 256,
    };
    c.bench_function("viewport_query_smoke_8k", |bench| {
        bench.iter(|| black_box(world.query(black_box(region)).len()));
    });
}

criterion_group!(timings, benchmarks);
criterion_main!(timings);
