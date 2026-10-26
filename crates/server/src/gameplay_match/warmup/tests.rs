use super::*;
use aoe_core::{PlayerId, WorldPosition};
use aoe_map::{MapPackage, MapRequest};
use aoe_simulation::StartSearchResult;

#[test]
fn startup_warms_only_the_village_neighbourhood_and_preserves_authoritative_state() {
    let package = MapPackage::new(1, MapRequest::default(), Vec::new()).unwrap();
    let mut world = GameWorld::from_map(package).unwrap();
    let StartSearchResult::Found(tile) = world
        .terrain()
        .search_start_checked(world.config(), 64, || false)
        .unwrap()
    else {
        panic!("a starting tile");
    };
    let unit = world
        .spawn_unit(PlayerId(0), WorldPosition::from_tile_center(tile).unwrap())
        .unwrap();
    let before = world.canonical_hash();
    let count = prepare(&world, &[unit], &|| false).unwrap();
    assert!((1..=9).contains(&count));
    let Terrain::Map { generator, .. } = world.terrain() else {
        unreachable!()
    };
    assert_eq!(generator.walk_cache_usage().1, count);
    assert!(generator.walk_chunk_resident(tile.x.div_euclid(32), tile.y.div_euclid(32)));
    assert_eq!(
        world.canonical_hash(),
        before,
        "warmup changed replay state"
    );
    assert_eq!(
        prepare(&world, &[unit], &|| true),
        Err(GameWorldError::Environment(
            aoe_map::EnvironmentPageError::Cancelled
        ))
    );
    assert_eq!(world.canonical_hash(), before);
}

#[test]
fn initial_camera_and_human_start_are_ready_before_the_match_opens() {
    // Flat source-backed ground isolates camera warming from the fallback's
    // cliffs. Use real village placement to obtain legal off-centre scouts.
    let (package, pages) =
        crate::gameplay_match::flat::package(crate::gameplay_match::flat::FlatMap {
            tiles_per_side: 512,
            seed: 1,
        })
        .unwrap();
    let hash = package.content_hash_hex();
    let mut world =
        GameWorld::from_prepared_map(package, pages, Vec::new(), Vec::new(), Vec::new()).unwrap();
    let unit = world
        .initialize_village(PlayerId(0), aoe_core::TileCoord::new(80, 80))
        .unwrap();
    let ai = world
        .initialize_village(PlayerId(1), aoe_core::TileCoord::new(432, 432))
        .unwrap();
    let tile = world.unit(unit).unwrap().position.tile_floor();
    let ai_tile = world.unit(ai).unwrap().position.tile_floor();
    let before = world.canonical_hash();
    let mut cache = camera(&world, &[unit, ai], &[true, false], &hash, &|| false).unwrap();
    assert!(cache.usage().entries > 25 && cache.usage().entries <= 50);
    assert!(
        cache
            .get(&crate::terrain_cache::Key {
                content_hash: hash.clone(),
                x: ai_tile.x / 32,
                y: ai_tile.y / 32,
            })
            .is_none(),
        "AI cameras must not expand the initial rendered warmup"
    );
    let side = world.config().width_tiles;
    for (x, y) in [(tile.x / 32, tile.y / 32), (side / 2 / 32, side / 2 / 32)] {
        let compact = cache
            .get(&crate::terrain_cache::Key {
                content_hash: hash.clone(),
                x,
                y,
            })
            .expect("the initial map-centre and human-start cameras were warmed");
        let Terrain::Map { generator, .. } = world.terrain() else {
            unreachable!()
        };
        assert_eq!(
            compact.decode().unwrap(),
            generator
                .landscape_chunk_with_cancel(x, y, &|| false)
                .unwrap()
        );
    }
    assert_eq!(world.canonical_hash(), before);
    assert!(camera(&world, &[unit, ai], &[true, false], &hash, &|| true).is_err());
}
