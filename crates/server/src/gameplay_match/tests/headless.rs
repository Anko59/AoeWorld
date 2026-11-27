//! The headless match: `POST /match` without HTTP, a socket or a clock.
//!
//! The interface these tests pin, for the implementer:
//! - `HeadlessMatch::start(&HeadlessPlan)` loads the plan's package directory,
//!   builds a match of `seats` AI slots on the package named by `map_hash`
//!   (seed `seed`), with the same `launch` and `install` that `POST /match`
//!   runs, and seats every slot with the Rust AI.
//! - `HeadlessMatch::tick()` is one `GameplayService::tick`.
//! - `HeadlessMatch::order_log()` has one line per order the seats had applied
//!   when the plan says `record_orders`.
use super::*;
use aoe_map::{MAP_SCHEMA_VERSION, MapPackage, MapRequest, Ratio};

/// A small synthetic package stored in a fresh directory, and its hash.
fn stored_package() -> (tempfile::TempDir, String) {
    let request = MapRequest {
        requested_side_meters: 1_000,
        compression: Ratio::new(1, 1).expect("ratio"),
        ..MapRequest::default()
    };
    let package = MapPackage::new(MAP_SCHEMA_VERSION, request, Vec::new()).expect("package");
    let directory = tempfile::tempdir().expect("directory");
    map_store::persist(Some(directory.path()), &package).expect("package stored");
    (directory, package.content_hash_hex())
}

fn plan(directory: &tempfile::TempDir, hash: &str, seed: u64) -> HeadlessPlan {
    HeadlessPlan {
        package_directory: directory.path().to_owned(),
        map_hash: hash.to_owned(),
        seats: 3,
        seed,
        late_game_percent: None,
        record_orders: true,
    }
}

/// The match at `seed`, ticked `ticks` times: its order log, world hash and tick.
async fn played(seed: u64, ticks: u32) -> (Vec<String>, [u8; 32], u64) {
    let (directory, hash) = stored_package();
    let game = HeadlessMatch::start(&plan(&directory, &hash, seed))
        .await
        .expect("a headless match on a stored package");
    for _ in 0..ticks {
        game.tick().await;
    }
    let (hash, tick) = game
        .gameplay()
        .inspect_scale_world(|world| (world.canonical_hash(), world.tick().0))
        .await;
    (game.order_log(), hash, tick)
}

#[tokio::test]
async fn every_slot_is_an_ai_seat_and_each_call_is_one_tick() {
    let (directory, hash) = stored_package();
    let game = HeadlessMatch::start(&plan(&directory, &hash, 7))
        .await
        .expect("a headless match");
    let slots = game.gameplay().slot_infos().await;
    assert_eq!(slots.len(), 3);
    assert!(slots.iter().all(|slot| !slot.human), "a slot is a person's");
    let before = game
        .gameplay()
        .inspect_scale_world(|world| world.tick().0)
        .await;
    for _ in 0..150 {
        game.tick().await;
    }
    let (after, units) = game
        .gameplay()
        .inspect_scale_world(|world| (world.tick().0, world.unit_count()))
        .await;
    assert_eq!(after, before + 150);
    assert!(units > 0);
    let log = game.order_log();
    assert!(!log.is_empty(), "the seats gave no orders");
    let seated: std::collections::BTreeSet<_> = log
        .iter()
        .filter_map(|line| line.split(' ').nth(1))
        .collect();
    assert!(seated.is_subset(&["0", "1", "2"].into()), "{seated:?}");
}

#[tokio::test]
async fn one_seed_gives_one_match_and_another_seed_another() {
    let first = played(7, 150).await;
    let second = played(7, 150).await;
    assert!(!first.0.is_empty());
    assert_eq!(first.0, second.0, "the order logs differ");
    assert_eq!(first.1, second.1, "the world hashes differ");
    let other = played(8, 150).await;
    assert_ne!(first.1, other.1, "the seed changed nothing");
}

#[tokio::test]
async fn a_package_that_is_not_in_the_directory_is_refused_by_name() {
    let (directory, _) = stored_package();
    let missing = "ab".repeat(32);
    let error = HeadlessMatch::start(&plan(&directory, &missing, 7))
        .await
        .err()
        .expect("a package that is not stored");
    assert!(error.to_string().contains(&missing), "{error}");
}

/// Units alive when the match has just started (no tick played).
async fn units_at_start(plan: &HeadlessPlan) -> usize {
    let game = HeadlessMatch::start(plan).await.expect("a headless match");
    game.gameplay()
        .inspect_scale_world(|world| world.unit_count())
        .await
}

#[tokio::test]
async fn the_late_game_recipe_seeds_every_village_before_the_first_tick() {
    let (directory, hash) = stored_package();
    let fresh = plan(&directory, &hash, 7);
    let late = HeadlessPlan {
        late_game_percent: Some(10),
        ..fresh.clone()
    };
    let (before, after) = (units_at_start(&fresh).await, units_at_start(&late).await);
    assert!(
        after > before + 30,
        "the recipe placed {} units over a fresh start of {before}",
        after - before.min(after)
    );
    let bigger = HeadlessPlan {
        late_game_percent: Some(20),
        ..fresh
    };
    assert!(
        units_at_start(&bigger).await > after,
        "the scale changed nothing"
    );
}

#[tokio::test]
async fn a_late_game_match_is_played_by_the_ai_and_stays_the_same_game() {
    let (directory, hash) = stored_package();
    let late = HeadlessPlan {
        late_game_percent: Some(10),
        ..plan(&directory, &hash, 7)
    };
    let mut hashes = Vec::new();
    for _ in 0..2 {
        let game = HeadlessMatch::start(&late)
            .await
            .expect("a late-game match");
        for _ in 0..60 {
            game.tick().await;
        }
        let hash = game
            .gameplay()
            .inspect_scale_world(|world| world.canonical_hash())
            .await;
        hashes.push((hash, game.order_log()));
    }
    assert_eq!(hashes[0], hashes[1], "one late-game seed played two games");
    assert!(!hashes[0].1.is_empty(), "the seats gave no orders");
}

#[tokio::test]
async fn a_bare_world_is_the_package_without_a_village_a_unit_or_a_seat() {
    let (directory, hash) = stored_package();
    let mut world = HeadlessMatch::bare_world(directory.path(), &hash)
        .await
        .expect("the world of a stored package");
    assert_eq!(world.unit_count(), 0, "a bare world holds no unit");
    assert_eq!(
        world.buildings().count(),
        0,
        "a bare world holds no building"
    );
    assert_eq!(world.tick().0, 0);
    for _ in 0..5 {
        world.advance();
    }
    assert_eq!(
        world.tick().0,
        5,
        "the caller ticks it, as the headless match does"
    );
    assert_eq!(world.unit_count(), 0, "nobody plays in it");
}

#[tokio::test]
async fn a_bare_world_of_a_package_that_is_not_stored_is_refused_by_name() {
    let (directory, _) = stored_package();
    let missing = "cd".repeat(32);
    let error = HeadlessMatch::bare_world(directory.path(), &missing)
        .await
        .expect_err("a package that is not stored");
    assert!(error.to_string().contains(&missing), "{error}");
}
