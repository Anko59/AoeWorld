//! Navigation replay must not depend on how many Rayon workers run it. The
//! 2048-tile, 7,000-unit measurement in `performance` proves this post-merge
//! alongside its timing report; this is the per-PR check, small enough to run
//! on every pull request.
use super::*;
use aoe_core::{PlayerId, TileCoord, WorldPosition};

/// Lazy and installed navigation both build caches, portals, components, the
/// hierarchy and pockets from parallel passes, and none of that may reach the
/// canonical hash. A 512-tile world with ten dispersed players still drives
/// every one of those phases; what it gives up against the measurement is
/// scale and the timing report, not a code path.
fn agrees_across_workers(installed: bool) {
    let run = |workers: usize| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = fixture::flat_world(512);
                let Terrain::Map { generator, .. } = world.terrain() else {
                    panic!("the fixture world is generated")
                };
                let directory = tempfile::tempdir().unwrap();
                if installed {
                    prepare(generator, [91; 32], Some(directory.path()), &|| false).unwrap();
                }
                world.set_fog_disabled(true);
                world.prepare_navigation(&|| false).unwrap();
                let mut units = Vec::new();
                for player in 0..10 {
                    let base = TileCoord::new(20 + player % 5 * 90, 20 + player / 5 * 90);
                    for slot in 0..10 {
                        let tile = TileCoord::new(base.x + slot % 5, base.y + slot / 5);
                        let at = WorldPosition::from_tile_center(tile).unwrap();
                        let id = world.spawn_unit(PlayerId(player as u16), at).unwrap();
                        units.push((id, tile));
                    }
                }
                // Cold, geographically dispersed routes keep the parallel
                // search and portal passes busy for the whole window.
                for tick in 0..40 {
                    for offset in 0..5 {
                        let (id, tile) = units[(tick * 7 + offset * 23) % units.len()];
                        let goal =
                            TileCoord::new((tile.x + 211) % 480 + 10, (tile.y + 157) % 480 + 10);
                        world
                            .issue_move(id, WorldPosition::from_tile_center(goal).unwrap())
                            .unwrap();
                    }
                    world.advance();
                }
                // Agreement proves nothing if nothing was planned or moved, so
                // the window must have done real navigation work.
                let moved = units
                    .iter()
                    .filter(|(id, tile)| {
                        world
                            .unit(*id)
                            .is_some_and(|u| u.position.tile_floor() != *tile)
                    })
                    .count();
                assert!(moved >= 50, "only {moved} of 100 units left their tile");
                world.canonical_hash()
            })
    };
    assert_eq!(
        run(1),
        run(4),
        "navigation replay diverged between one and four workers (installed: {installed})"
    );
}

#[test]
fn lazy_navigation_replay_agrees_across_workers() {
    agrees_across_workers(false);
}

#[test]
fn installed_navigation_replay_agrees_across_workers() {
    agrees_across_workers(true);
}
