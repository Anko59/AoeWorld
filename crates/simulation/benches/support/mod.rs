//! Shared construction for the complete and focused late-game kernels.
use aoe_core::{WorldConfig, WorldPosition};
use aoe_scenario::{SMOKE, Scenario, TARGET_DISTRIBUTED};
use aoe_simulation::GameWorld;

/// One worker makes instruction/allocation counts independent of runner CPU
/// count. The calling thread is the worker, avoiding a timed cross-thread
/// handoff. Timing profiles use the ordinary parallel path.
pub fn single_worker() {
    static POOL: std::sync::Once = std::sync::Once::new();
    POOL.call_once(|| {
        if let Err(error) = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .use_current_thread()
            .build_global()
        {
            eprintln!("benchmark worker setup failed: {error}");
            std::process::exit(1);
        }
    });
}

/// The `perf-smoke`/`perf-ci` offline population: the scenario's units, the
/// first 1% (at most 512) ordered to move. The measured ticks follow.
#[allow(dead_code)]
pub fn setup_population(scenario: Scenario) -> GameWorld {
    single_worker();
    let config = WorldConfig {
        width_tiles: scenario.world_size,
        height_tiles: scenario.world_size,
        seed: scenario.seed,
        ..WorldConfig::default()
    };
    let mut world = match GameWorld::with_population_in_extent(
        config,
        scenario.entities,
        scenario.hotspot_entities,
        scenario.players,
        scenario.active_extent,
    ) {
        Ok(world) => world,
        Err(error) => {
            eprintln!("population workload failed: {error}");
            std::process::exit(1);
        }
    };
    let moving = world
        .units()
        .take((scenario.entities as usize / 100).min(512))
        .map(|unit| (unit.id, unit.position))
        .collect::<Vec<_>>();
    for (id, position) in moving {
        let destination = WorldPosition::new(
            position.x.saturating_add(4_096),
            position.y.saturating_add(2_048),
        );
        let _ = world.issue_move(id, destination);
    }
    world
}

/// The four ticks `perf-ci` times for a population workload, as it measures
/// them; setup and teardown stay outside the instrumented region.
#[allow(dead_code)]
pub fn four_ticks(mut world: GameWorld) -> GameWorld {
    gungraun::client_requests::callgrind::start_instrumentation();
    for _ in 0..4 {
        world.advance();
    }
    gungraun::client_requests::callgrind::stop_instrumentation();
    std::hint::black_box(world)
}

#[allow(dead_code)]
pub fn setup_smoke() -> GameWorld {
    setup_population(SMOKE)
}

#[allow(dead_code)]
pub fn setup_distributed() -> GameWorld {
    setup_population(TARGET_DISTRIBUTED)
}

pub fn setup_late_game() -> GameWorld {
    single_worker();
    eprintln!("late-game instruction setup: generating 100 seats");
    let mut world = match GameWorld::late_game_workload() {
        Ok(world) => world,
        Err(error) => {
            eprintln!("late-game workload failed: {error}");
            std::process::exit(1);
        }
    };
    eprintln!("late-game instruction setup: warming 384 ticks");
    for _ in 0..384 {
        world.advance();
    }
    assert_eq!(world.players().count(), 100);
    assert!((13_000..=15_000).contains(&world.unit_count()));
    assert!(
        world
            .units()
            .filter(|u| u.task == aoe_simulation::UnitTask::Gathering)
            .count()
            >= 1_000
    );
    assert!(
        world
            .units()
            .filter(|u| u.task == aoe_simulation::UnitTask::Returning)
            .count()
            >= 100
    );
    assert_eq!(
        world
            .buildings()
            .filter(|b| !b.training_queue.is_empty())
            .count(),
        200
    );
    assert_eq!(world.units().filter(|u| u.auto_scout).count(), 100);
    assert_eq!(world.units().filter(|u| u.kind.is_naval()).count(), 200);
    assert_eq!(
        world
            .units()
            .filter(|u| world.formation(u.id).is_some())
            .count(),
        1_200
    );
    assert_eq!(
        world.buildings().map(|b| b.garrison.len()).sum::<usize>(),
        100
    );
    assert!(
        world
            .buildings()
            .filter(|b| b
                .training_queue
                .front()
                .is_some_and(|e| e.progress_ticks == e.total_ticks))
            .count()
            >= 15
    );
    assert!(
        world
            .units()
            .filter(|u| u.moving && u.position != u.previous_position)
            .count()
            >= 2_000
    );
    eprintln!("late-game instruction setup: ready");
    world
}
