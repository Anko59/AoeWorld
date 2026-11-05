//! The France march scoreboard: real package terrain, no browser. Armies march
//! between chosen tiles and are judged against the true walking distance.
//!
//! On demand: `.cache/armies/france/enabled` names the output label; without it
//! (or without the package in `local-assets/`, which never enters Git) every test
//! returns at once. Run one filter:
//! `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-server
//! AOE_NATIVE_TEST_FILTER='test(france_march::table::)'`, narrowed by pair or army name.
use crate::map_store::{PageResidency, navigation};
use aoe_core::{BridgeSpan, TileCoord};
use aoe_map::{MapPackage, WalkingField, walking_field};
use aoe_simulation::{GameWorld, Terrain};
use std::path::PathBuf;

mod run;
mod survey;
mod table;

const FRANCE: &str = "dc7e0d4b253af4e7e7106b69eba61169ef2f8f6b0fd7781fbe5a3ed6ccc60e68";

fn root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn output() -> PathBuf {
    root().join(".cache/armies/france")
}

/// The output label, or nothing when the scoreboard is not asked for.
fn label() -> Option<String> {
    let label = std::fs::read_to_string(output().join("enabled")).ok()?;
    let packages = root().join("local-assets/map-packages");
    packages
        .join(format!("{FRANCE}.json"))
        .exists()
        .then(|| label.trim().to_owned())
}

/// France exactly as a served match loads it, without its starting village.
fn france() -> GameWorld {
    let directory = root().join("local-assets/map-packages");
    let bytes = std::fs::read(directory.join(format!("{FRANCE}.json"))).unwrap();
    let package: MapPackage = serde_json::from_slice(&bytes).unwrap();
    let hash = package.content_hash;
    let provider = PageResidency::open(&directory, &package, &|| false).unwrap();
    let mut world = GameWorld::from_page_provider(package, provider).unwrap();
    navigation::prepare_world(&world, hash, Some(&directory), &|| false).unwrap();
    world.set_fog_disabled(true);
    world
}

fn span(from: (i32, i32), to: (i32, i32)) -> BridgeSpan {
    BridgeSpan::new(TileCoord::new(from.0, from.1), TileCoord::new(to.0, to.1)).unwrap()
}

/// Walking costs (1024 a tile) from `goal`, within `margin` tiles of the two ends.
fn field(world: &GameWorld, goal: TileCoord, start: TileCoord, margin: i32) -> WalkingField {
    let Terrain::Map { generator, overlay } = world.terrain() else {
        panic!("France is a map");
    };
    let side = world.config().width_tiles - 1;
    let min = TileCoord::new(
        (goal.x.min(start.x) - margin).max(0),
        (goal.y.min(start.y) - margin).max(0),
    );
    let max = TileCoord::new(
        (goal.x.max(start.x) + margin).min(side),
        (goal.y.max(start.y) + margin).min(side),
    );
    walking_field(generator, overlay, goal, min, max)
}
