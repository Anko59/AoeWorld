//! The replay must not depend on how many Rayon workers run it. The full
//! 100-seat workload proves this post-merge in
//! `economy::benchmark::tests`; this is the per-PR check, at eight seats, so a
//! parallel divergence is caught by every pull request in seconds.
use super::*;

/// Parallel iteration order must not reach the canonical hash. Eight seats
/// still run gatherers, formations and movers through the same parallel
/// movement and economy passes as a hundred do; what the small workload gives
/// up is scale, not a code path, so a divergence that needs 14,000 units to
/// appear is left to the post-merge tier.
#[test]
fn the_late_game_replay_agrees_across_one_and_four_workers() {
    fn run(threads: usize) -> [u8; 32] {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap()
            .install(|| {
                let mut world = GameWorld::small_late_game_workload().unwrap();
                for _ in 0..96 {
                    world.advance();
                }
                // The parallel passes must have real work in them, or agreement
                // would prove nothing.
                assert!(
                    world.economy.gatherers.len() >= 100,
                    "only {} gatherers",
                    world.economy.gatherers.len()
                );
                assert!(
                    world
                        .units()
                        .filter(|u| world.formation(u.id).is_some())
                        .count()
                        >= 32
                );
                let moving = world.units().filter(|u| u.moving).count();
                assert!(
                    moving >= 2 * crate::game_world::parallel::SHARD,
                    "only {moving} simultaneous movers; below 2 * SHARD the \
                     proposal pass runs sequentially and worker counts cannot differ"
                );
                world.canonical_hash()
            })
    }
    assert_eq!(
        run(1),
        run(4),
        "the eight-seat late-game replay diverged between one and four workers"
    );
}
