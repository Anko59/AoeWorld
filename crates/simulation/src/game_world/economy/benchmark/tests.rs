// Radius geometry, exact static sweeps and local rejoining intentionally
// change these current pins. Preserve workloads, floors and replay comparisons.
// Radius collision and persistent work reservations, bounded position searches
// and continuous yielding intentionally change this current replay checkpoint.
// Keep the same workload, activity floors and independent replay comparisons.
use super::*;
use crate::UnitTask;

#[test]
fn late_game_workload_keeps_real_jobs_and_full_queues() {
    let mut world = build(2).unwrap();
    assert_eq!(world.unit_count(), 280);
    assert_eq!(world.players().count(), 2);
    assert!(world.animals().count() >= 16);
    assert_eq!(
        world.buildings().filter(|b| b.research.is_some()).count(),
        2
    );
    assert_eq!(
        world
            .buildings()
            .filter(|b| !b.training_queue.is_empty())
            .count(),
        4
    );
    assert!(
        world
            .buildings()
            .filter(|b| !b.training_queue.is_empty())
            .all(|b| b.training_queue.len() == 5)
    );
    let initial = world.canonical_hash();
    let stock: u32 = world.players().map(|p| p.stockpile.wood).sum();
    for _ in 0..384 {
        world.advance();
    }
    assert!(world.economy.gatherers.len() >= 120);
    assert!(world.economy.tick_work.training_buildings >= 4);
    assert!(world.economy.tick_work.research_buildings >= 2);
    assert!(world.economy.tick_work.spawn_units > 0);
    assert!(world.units().filter(|u| u.moving).count() >= 10);
    assert!(world.units().any(|u| u.task == UnitTask::Gathering));
    assert!(
        world.players().map(|p| p.stockpile.wood).sum::<u32>() > stock,
        "villagers must deposit, not just hold gather orders"
    );
    assert_ne!(world.canonical_hash(), initial);
    assert_eq!(
        world
            .buildings()
            .filter(|b| !b.training_queue.is_empty())
            .count(),
        4
    );
}

#[test]
fn late_game_full_population_replays_at_the_profile_checkpoint() {
    fn run(threads: usize) -> [u8; 32] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        pool.install(|| {
            let mut world = GameWorld::late_game_workload().unwrap();
            assert_eq!(world.unit_count(), 14_000);
            assert_eq!(world.players().count(), 100);
            for _ in 0..384 {
                world.advance();
            }
            assert!(world.economy.gatherers.len() >= 6_000);
            assert!(
                world
                    .units()
                    .filter(|u| u.kind != crate::UnitKind::Villager && u.moving)
                    .count()
                    >= 500
            );
            world.canonical_hash()
        })
    }
    // Radius collision, persistent work reservations and bounded position
    // searches deliberately change this current checkpoint. Friendly
    // squeeze-through and worksite-only gather redirects change it again. Preserve the
    // exact workload, activity floors and complete 1/4-worker comparison.
    let single = run(1);
    assert_eq!(single, run(4));
    // Regenerate with the focused late_game_full_population_replays test.
    let checkpoint: String = single.iter().map(|byte| format!("{byte:02x}")).collect();
    assert_eq!(
        checkpoint,
        // Current combat stance and targeting state; 1/4-worker comparison above stays.
        "aac7700521f7c864c96df7b5ee84c8d43b0c2a2b1fd855d60032ca54a20f9b42"
    );
}

#[test]
fn late_game_exercises_exploration_marches_and_backlogged_production() {
    let mut world = build(2).unwrap();
    assert!(!world.fog_disabled());
    for player in world.players() {
        assert_eq!(player.age, crate::Age::Castle);
        assert_ne!(player.researched & Tech::FeudalAge.bit(), 0);
        assert_ne!(player.researched & Tech::CastleAge.bit(), 0);
    }
    assert!(world.units().filter(|u| u.auto_scout).count() >= 2);
    assert!(
        world
            .units()
            .filter(|u| u.kind == crate::UnitKind::Monk)
            .count()
            >= 2
    );
    assert!(world.units().filter(|u| u.kind.is_naval()).count() >= 4);
    assert!(
        world
            .buildings()
            .any(|b| b.kind == BuildingKind::WatchTower)
    );
    for _ in 0..384 {
        world.advance();
    }
    let blocked = world
        .buildings()
        .filter(|b| {
            b.training_queue
                .front()
                .is_some_and(|e| e.progress_ticks == e.total_ticks)
        })
        .count();
    assert!(blocked >= 1, "completed queues must retry occupied exits");
    assert!(world.buildings().any(|b| !b.garrison.is_empty()));
    assert!(world.units().any(|u| world.formation(u.id).is_some()));
    let mut walked = 0;
    for _ in 0..128 {
        world.advance();
        walked += world
            .units()
            .filter(|u| u.moving && u.position != u.previous_position)
            .count();
    }
    assert!(
        walked >= 128 * 40,
        "real displacement, not only outstanding orders: {walked}"
    );
    assert!(world.units().filter(|u| u.auto_scout).count() >= 2);
}

#[test]
fn late_game_all_seats_seed_the_requested_actors() {
    let world = build(100).unwrap();
    assert_eq!(world.unit_count(), 14_000);
    assert_eq!(world.units().filter(|u| u.auto_scout).count(), 100);
    assert_eq!(world.units().filter(|u| u.kind.is_naval()).count(), 200);
    assert_eq!(
        world
            .buildings()
            .filter(|b| b.kind == BuildingKind::WatchTower)
            .count(),
        100
    );
}
