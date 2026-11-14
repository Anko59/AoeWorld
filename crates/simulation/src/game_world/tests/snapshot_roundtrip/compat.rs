//! A change to the snapshot layout without a `WORLD_SNAPSHOT_VERSION` bump fails
//! here, in every pull request, instead of on someone's save later (2026-11-14:
//! #828 added a formation field at version 5 and every existing save stopped
//! loading; nothing noticed).
//!
//! Two checks for the current version, kept in `compat/`:
//! - `v<N>.schema`: a hash of the record definitions in `snapshot/records.rs`
//!   (names, order and types; comments ignored). It catches a layout change even
//!   when old bytes still decode, such as two swapped `u64` fields.
//! - `v<N>.postcard`: a snapshot of a small busy game, which must still decode and
//!   restore. It also catches changes in types defined outside `records.rs` that
//!   break decoding (an enum variant reordered elsewhere can still slip past both).
//!
//! After a bump the test records both files for the new version, removes the old
//! ones (no legacy before v1), and fails once until they are committed. A
//! wire-neutral edit (a rename, a derive, formatting) re-records only the schema
//! at the same version, since every bump makes existing saves unreadable; the
//! committed snapshot must still restore.
use super::*;
use crate::game_world::snapshot::WORLD_SNAPSHOT_VERSION;
use aoe_core::Formation;
use std::path::{Path, PathBuf};

const RECORDS: &str = include_str!("../../snapshot/records.rs");

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/game_world/tests/snapshot_roundtrip/compat")
}

/// The fixture's path from the repository root, as a person runs the test.
fn shown(path: &Path) -> String {
    path.strip_prefix(env!("CARGO_MANIFEST_DIR")).map_or_else(
        |_| path.display().to_string(),
        |p| format!("crates/simulation/{}", p.display()),
    )
}

/// FNV-1a over the record definitions with comments and spacing removed: stable
/// across Rust releases, unlike `DefaultHasher`.
fn schema() -> String {
    let text: String = RECORDS
        .lines()
        .map(|line| line.split("//").next().unwrap_or_default())
        .flat_map(str::split_whitespace)
        .collect::<Vec<_>>()
        .join(" ");
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{hash:016x}\n")
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
fn the_snapshot_layout_cannot_change_without_a_version_bump() {
    let version = WORLD_SNAPSHOT_VERSION;
    let schema_path = fixtures().join(format!("v{version}.schema"));
    let bytes_path = fixtures().join(format!("v{version}.postcard"));
    if !bytes_path.exists() {
        // A new version: record both files and drop the old ones.
        std::fs::create_dir_all(fixtures()).unwrap();
        for entry in std::fs::read_dir(fixtures()).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).unwrap();
        }
        let snapshot = busy().snapshot([0; 32]).expect("snapshot");
        std::fs::write(&bytes_path, postcard::to_allocvec(&snapshot).unwrap()).unwrap();
        std::fs::write(&schema_path, schema()).unwrap();
        panic!(
            "recorded {} and {} for version {version} and removed older fixtures: commit them",
            shown(&schema_path),
            shown(&bytes_path)
        );
    }
    if !schema_path.exists() {
        // A wire-neutral edit at the same version: the committed snapshot below
        // must still restore, which is what proves the bytes did not move.
        std::fs::write(&schema_path, schema()).unwrap();
        panic!(
            "re-recorded {} at version {version}: commit it if the snapshot below still restores",
            shown(&schema_path)
        );
    }
    let recorded = std::fs::read_to_string(&schema_path).unwrap();
    assert_eq!(
        recorded,
        schema(),
        "snapshot/records.rs changed at version {version}. If the encoding changed (a field \
         added, removed, moved or retyped), bump WORLD_SNAPSHOT_VERSION and rerun to record \
         both fixtures. If it was only a rename, a derive or formatting, with no field moved \
         or changing meaning, delete {} and rerun: the committed snapshot must still restore. \
         A bump makes every existing save unreadable",
        shown(&schema_path)
    );
    let bytes = std::fs::read(&bytes_path).unwrap();
    let decoded = postcard::from_bytes::<WorldSnapshot>(&bytes).unwrap_or_else(|error| {
        panic!(
            "{}: the snapshot no longer decodes ({error}), so the encoding changed: bump \
             WORLD_SNAPSHOT_VERSION and rerun to record both fixtures",
            shown(&bytes_path)
        )
    });
    flat()
        .0
        .restore_snapshot(&decoded, [0; 32], &|| false)
        .unwrap_or_else(|error| panic!("{}: does not restore: {error:?}", shown(&bytes_path)));
}
