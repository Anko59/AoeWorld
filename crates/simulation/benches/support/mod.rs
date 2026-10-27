//! Shared construction for the complete and focused late-game kernels.
use aoe_simulation::GameWorld;

pub fn setup_late_game() -> GameWorld {
    // One worker makes the instruction/allocation comparison independent of
    // runner CPU count. The calling thread is the worker, avoiding a timed
    // cross-thread handoff. Timing profiles use the ordinary parallel path.
    static POOL: std::sync::Once = std::sync::Once::new();
    POOL.call_once(|| {
        if let Err(error) = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .use_current_thread()
            .build_global()
        {
            eprintln!("late-game benchmark worker setup failed: {error}");
            std::process::exit(1);
        }
    });
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
