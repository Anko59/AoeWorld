use aoe_core::{PlayerId, Region, Seed, TileCoord, WorldConfig, WorldPosition};
use aoe_scenario::SMOKE;
use aoe_simulation::{GameWorld, World};
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
    let result = combat_approaches(c);
    assert!(result.is_ok(), "combat fixture: {result:?}");
}

fn combat_approaches(c: &mut Criterion) -> Result<(), Box<dyn std::error::Error>> {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7))?)?;
    let target = world.place_town_center(PlayerId(1), TileCoord::new(110, 100))?;
    let mut attackers = Vec::new();
    for i in 0..16 {
        attackers.push(world.spawn_militia(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(100 + i % 4, 100 + i / 4))?,
        )?);
    }
    c.bench_function("combat_attack_positions_16", |bench| {
        bench.iter(|| {
            for &id in &attackers {
                let result = world.issue_attack(PlayerId(0), id, black_box(target));
                assert!(result.is_ok(), "approach order: {result:?}");
            }
        });
    });
    Ok(())
}

criterion_group!(timings, benchmarks);
criterion_main!(timings);
