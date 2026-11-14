//! A committed snapshot per format version: a change to the snapshot's layout
//! without a `WORLD_SNAPSHOT_VERSION` bump fails here, in every pull request, not
//! later on someone's save (2026-11-14: #828 added a formation field at version 5
//! and every existing save stopped loading).
//!
//! The fixture for the current version must still decode and restore. One for an
//! older version must be refused. With no fixture for the current version (right
//! after a bump), the test records one and fails until it is committed.
use super::*;
use crate::game_world::snapshot::WORLD_SNAPSHOT_VERSION;
use aoe_core::Formation;
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/game_world/tests/snapshot_roundtrip/compat")
}

fn fixture(version: u16) -> PathBuf {
    fixtures().join(format!("v{version}.postcard"))
}

/// The fixture's path from the repository root, as a person runs the test.
fn shown(path: &std::path::Path) -> String {
    path.strip_prefix(env!("CARGO_MANIFEST_DIR")).map_or_else(
        |_| path.display().to_string(),
        |p| format!("crates/simulation/{}", p.display()),
    )
}

/// A small game with the records a save carries: a village at work, buildings,
/// an army in formation on an attack-move, and a scout with a queued leg.
fn busy() -> GameWorld {
    let (mut world, start) = flat();
    world.initialize_village(PlayerId(0), start).unwrap();
    let at = |dx: i32, dy: i32| {
        aoe_core::WorldPosition::from_tile_center(TileCoord::new(start.x + dx, start.y + dy))
            .unwrap()
    };
    let army: Vec<_> = (0..6)
        .map(|i| {
            world
                .spawn_kind(PlayerId(0), crate::UnitKind::Militia, at(30 + i, 30))
                .unwrap()
        })
        .collect();
    world
        .issue_formation_move(PlayerId(0), &army, Formation::Line, at(44, 30), false, true)
        .unwrap();
    let scout = world
        .spawn_kind(PlayerId(0), crate::UnitKind::Scout, at(30, 36))
        .unwrap();
    world.issue_move(scout, at(40, 36)).unwrap();
    world.issue_queued_move(scout, at(46, 40)).unwrap();
    for _ in 0..40 {
        world.advance();
    }
    world
}

#[test]
fn every_snapshot_format_has_a_fixture_and_old_ones_are_refused() {
    let current = fixture(WORLD_SNAPSHOT_VERSION);
    if !current.exists() {
        let snapshot = busy().snapshot([0; 32]).expect("snapshot");
        std::fs::create_dir_all(fixtures()).unwrap();
        std::fs::write(&current, postcard::to_allocvec(&snapshot).unwrap()).unwrap();
        panic!(
            "recorded {}: commit it with the WORLD_SNAPSHOT_VERSION bump",
            shown(&current)
        );
    }
    for entry in std::fs::read_dir(fixtures()).unwrap() {
        let path = entry.unwrap().path();
        let bytes = std::fs::read(&path).unwrap();
        // The version is the snapshot's first field, so it reads before the body.
        let (version, _) = postcard::take_from_bytes::<u16>(&bytes).unwrap();
        let decoded = postcard::from_bytes::<WorldSnapshot>(&bytes);
        if version == WORLD_SNAPSHOT_VERSION {
            let decoded = decoded.unwrap_or_else(|error| {
                panic!(
                    "{}: the snapshot format changed without a version bump ({error}); \
                     bump WORLD_SNAPSHOT_VERSION, run this test to record the new fixture, \
                     and commit it",
                    shown(&path)
                )
            });
            flat()
                .0
                .restore_snapshot(&decoded, [0; 32], &|| false)
                .unwrap_or_else(|error| panic!("{}: does not restore: {error:?}", shown(&path)));
        } else {
            assert!(
                version < WORLD_SNAPSHOT_VERSION,
                "{}: from a newer build",
                shown(&path)
            );
            // An older format either fails to decode or is refused by its version;
            // it must never restore as if it were current.
            if let Ok(decoded) = decoded {
                let refused = flat().0.restore_snapshot(&decoded, [0; 32], &|| false);
                assert!(
                    matches!(refused, Err(SnapshotError::Version)),
                    "{}: an old snapshot was not refused: {refused:?}",
                    shown(&path)
                );
            }
        }
    }
}
