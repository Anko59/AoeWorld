use super::super::{EdgePassability, GroundMaterial, MapChunkGenerator};
use aoe_core::TileCoord;

fn direct(generator: &MapChunkGenerator, tile: TileCoord) -> Option<super::WalkCell> {
    generator
        .landscape_point_with_cancel(tile, &|| false)
        .expect("direct")
        .map(|point| super::WalkCell {
            passable: point.tile.terrain.passable,
            walkable_surface: point.tile.terrain.surface.walkable(),
            game_height_level: point.tile.terrain.game_height_level,
            mud: point.tile.terrain.material == GroundMaterial::Mud,
            node: point.resource.map(|resource| resource.node),
        })
}

#[test]
fn chunk_cells_and_edges_equal_direct_evaluation_including_the_map_edge() {
    // 100 tiles: the last chunk row and column are partly outside the map.
    let generator = MapChunkGenerator::new([7; 32], 11, 100);
    for cy in 0..4 {
        for cx in 0..4 {
            generator.precompose_walk_chunk(cx, cy, &|| false).unwrap();
        }
    }
    for y in -2..103 {
        for x in -2..103 {
            let tile = TileCoord::new(x, y);
            let direct_point = generator
                .landscape_point_with_cancel(tile, &|| false)
                .unwrap()
                .map(|point| {
                    (
                        point.tile.terrain,
                        point.resource.map(|resource| resource.node),
                    )
                });
            assert_eq!(
                generator
                    .tile_and_node_with_cancel(tile, &|| false)
                    .unwrap(),
                direct_point
            );
            assert_eq!(
                generator.walk_at_with_cancel(tile, &|| false),
                Ok(direct(&generator, tile)),
                "{tile:?}"
            );
            for (dx, dy) in [(1, 0), (0, 1), (1, 1), (-1, 1)] {
                let next = TileCoord::new(x + dx, y + dy);
                let cached = generator.edge_between_with_cancel(tile, next, &|| false);
                let (Some(a), Some(b)) = (
                    generator
                        .base_physical_tile_with_cancel(tile, &|| false)
                        .unwrap(),
                    generator
                        .base_physical_tile_with_cancel(next, &|| false)
                        .unwrap_or(None),
                ) else {
                    assert!(cached.is_err());
                    continue;
                };
                let expected = if !a.passable
                    || !b.passable
                    || !a.surface.walkable()
                    || !b.surface.walkable()
                    || (i32::from(a.game_height_level) - i32::from(b.game_height_level)).abs() > 1
                {
                    EdgePassability::Blocked
                } else {
                    EdgePassability::Passable
                };
                assert_eq!(cached, Ok(expected), "{tile:?} {next:?}");
            }
        }
    }
}

#[test]
fn cache_is_bounded_and_cancellation_wins_over_a_hit() {
    let generator = MapChunkGenerator::new([3; 32], 5, 4_096);
    let tile = TileCoord::new(40, 40);
    generator.precompose_walk_chunk(1, 1, &|| false).unwrap();
    assert!(generator.walk_at_with_cancel(tile, &|| false).is_ok());
    let (bytes, chunks) = generator.walk_cache_usage();
    assert_eq!(chunks, 1);
    assert!(bytes > 0 && bytes < super::WALK_CACHE_MAX_BYTES);
    assert!(generator.walk_at_with_cancel(tile, &|| true).is_err());
    let mut cache = super::WalkCache::default();
    let probe = generator
        .walk_chunk((0, 0), &|| false)
        .expect("chunk")
        .expect("built");
    let per_chunk = probe.bytes();
    for index in 0..(super::WALK_CACHE_MAX_BYTES / per_chunk + 50) as i32 {
        cache.insert((index, 0), probe.clone());
    }
    assert!(cache.retained_bytes() <= super::WALK_CACHE_MAX_BYTES);
    assert!(cache.get((0, 0)).is_none(), "oldest chunk is evicted first");
}

#[test]
fn navigation_spans_balance_and_warm_walk_hits_do_not_compose() {
    use crate::{NavigationPhase, NavigationProbe, NavigationSpan};
    use std::sync::{Arc, Mutex};
    #[derive(Debug)]
    struct Probe(Arc<Mutex<Vec<(NavigationPhase, bool)>>>);
    struct Span(Arc<Mutex<Vec<(NavigationPhase, bool)>>>, NavigationPhase);
    impl NavigationSpan for Span {}
    impl Drop for Span {
        fn drop(&mut self) {
            self.0.lock().unwrap().push((self.1, false));
        }
    }
    impl NavigationProbe for Probe {
        fn begin(&self, phase: NavigationPhase) -> Box<dyn NavigationSpan> {
            self.0.lock().unwrap().push((phase, true));
            Box::new(Span(self.0.clone(), phase))
        }
    }
    let generator = MapChunkGenerator::new([3; 32], 5, 128);
    let events = Arc::new(Mutex::new(Vec::new()));
    generator.set_navigation_probe(Arc::new(Probe(events.clone())));
    let tile = TileCoord::new(40, 40);
    let cold = generator.walk_at_with_cancel(tile, &|| false).unwrap();
    assert_eq!(
        *events.lock().unwrap(),
        vec![
            (NavigationPhase::TerrainPoint, true),
            (NavigationPhase::TerrainPoint, false),
        ]
    );
    events.lock().unwrap().clear();
    assert_eq!(
        generator.walk_at_with_cancel(tile, &|| false).unwrap(),
        cold
    );
    assert!(events.lock().unwrap().is_empty());
    generator.precompose_walk_chunk(2, 2, &|| false).unwrap();
    assert!(
        events.lock().unwrap().is_empty(),
        "background preparation must not be charged to a tick"
    );
    assert!(generator.walk_chunk_resident(2, 2));
    assert!(
        generator
            .walk_at_with_cancel(TileCoord::new(80, 80), &|| true)
            .is_err()
    );
    assert!(events.lock().unwrap().is_empty());
}

#[test]
fn a_cold_single_tile_query_does_not_compose_a_whole_chunk() {
    let generator = MapChunkGenerator::new([3; 32], 5, 128);
    let tile = TileCoord::new(40, 40);
    assert_eq!(
        generator.walk_at_with_cancel(tile, &|| false).unwrap(),
        direct(&generator, tile)
    );
    assert_eq!(
        generator.walk_cache_usage(),
        (0, 0),
        "single-tile validation must not synchronously compose 1024 tiles"
    );
}

#[test]
fn prepared_chunk_serves_full_points_without_recomposing_or_using_the_point_memo() {
    let generator = MapChunkGenerator::new([3; 32], 5, 128);
    let tile = TileCoord::new(40, 40);
    let expected = generator
        .tile_and_node_with_cancel(tile, &|| false)
        .unwrap();
    generator.precompose_walk_chunk(1, 1, &|| false).unwrap();
    *generator.derived.point.lock().unwrap() = Default::default();
    assert_eq!(
        generator
            .tile_and_node_with_cancel(tile, &|| false)
            .unwrap(),
        expected
    );
    assert_eq!(
        generator.derived.point.lock().unwrap().get(tile),
        None,
        "a resident chunk must serve full points without another composition"
    );
    assert!(generator.tile_and_node_with_cancel(tile, &|| true).is_err());
}
