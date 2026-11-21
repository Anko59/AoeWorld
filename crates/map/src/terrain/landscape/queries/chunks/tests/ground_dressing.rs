use super::*;

#[test]
fn ground_variety_changes_only_the_drawn_sheet_and_stays_in_its_family() {
    use GroundMaterial::{Dirt, DryGrass, GrassDirt, Gravel, Sand, WornGrass};
    let families: [(u8, &[GroundMaterial]); 2] = [
        (20, &[DryGrass, GrassDirt, Dirt, WornGrass]),
        (27, &[Sand, Gravel, Dirt]),
    ];
    for (class, family) in families {
        let generator = fixture(512, class, 20_000);
        let mut seen = Vec::new();
        for (x, y) in [(0, 0), (3, 5), (7, 7)] {
            let chunk = candidate(&generator, x, y);
            assert_eq!(chunk.tiles.len(), 1024);
            let stones = (y - 1..=y + 1)
                .flat_map(|cy| (x - 1..=x + 1).map(move |cx| (cx, cy)))
                .filter(|(cx, cy)| *cx >= 0 && *cy >= 0)
                .flat_map(|(cx, cy)| {
                    generator
                        .evaluate_landscape_chunk_with_cancel(
                            cx,
                            cy,
                            &policy,
                            &|_| Reservations::default(),
                            &|| false,
                        )
                        .unwrap()
                        .into_iter()
                        .flat_map(|neighbor| neighbor.decorations)
                })
                .filter(|d| d.family == DecorationFamily::Stone)
                .collect::<Vec<_>>();
            for tile in &chunk.tiles {
                let mut base = generator
                    .landscape_base_at(tile.tile, &|| false)
                    .expect("source")
                    .expect("in bounds");
                // Include anchors owned by the adjacent chunks.
                base.passable &= !stones.iter().any(|rock| {
                    rock.tile.x.abs_diff(tile.tile.x) <= 1 && rock.tile.y.abs_diff(tile.tile.y) <= 1
                });
                let mut physical = tile.terrain;
                physical.material = base.material;
                assert_eq!(physical, base);
                assert!(family.contains(&tile.terrain.material), "class {class}");
                if !seen.contains(&tile.terrain.material) {
                    seen.push(tile.terrain.material);
                }
                // Seamless: a single point query draws the same sheet.
                if tile.tile.x % 32 == 31 || tile.tile.y % 32 == 0 {
                    let point = generator
                        .evaluate_landscape_point_with_cancel::<false>(
                            tile.tile,
                            &policy,
                            &|_| Reservations::default(),
                            &|| false,
                        )
                        .expect("source")
                        .expect("in bounds");
                    assert_eq!(point.tile, *tile);
                }
            }
        }
        assert!(seen.len() >= 3, "class {class} shows {seen:?}");
    }
}

#[test]
fn meadow_dressing_forms_noticeable_patches_and_preserves_physics() {
    let generator = fixture(128, 8, 100);
    let meadow = |_: TileCoord, _: Tile| LandscapePolicy {
        wildness_per_thousand: Some(0),
        support_per_thousand: 0,
    };
    for (cx, cy) in [(0, 0), (1, 1), (2, 2)] {
        let scene = generator
            .evaluate_landscape_chunk_with_cancel(
                cx,
                cy,
                &meadow,
                &|_| Reservations::default(),
                &|| false,
            )
            .unwrap()
            .unwrap();
        let vegetation = scene
            .decorations
            .iter()
            .filter(|d| d.family != DecorationFamily::Stone && d.family != DecorationFamily::Cliff)
            .collect::<Vec<_>>();
        assert!(
            (80..240).contains(&vegetation.len()),
            "noticeable meadow: {}",
            vegetation.len()
        );
        let clustered = vegetation
            .iter()
            .filter(|d| {
                vegetation.iter().any(|other| {
                    other.tile != d.tile
                        && (other.tile.x - d.tile.x).abs() <= 2
                        && (other.tile.y - d.tile.y).abs() <= 2
                })
            })
            .count();
        assert!(clustered * 4 > vegetation.len() * 3, "natural patches");
        for d in &scene.decorations {
            assert!(!scene.resources.iter().any(|r| r.node.tile == d.tile));
            let point = generator
                .evaluate_landscape_point_with_cancel::<false>(
                    d.tile,
                    &meadow,
                    &|_| Reservations::default(),
                    &|| false,
                )
                .unwrap()
                .unwrap();
            assert_eq!(point.decoration, Some(*d), "chunk traversal and boundaries");
            assert_eq!(
                point.tile.terrain.surface,
                generator.sample_base_tile(d.tile).surface
            );
            assert_eq!(
                point.tile.terrain.passable,
                generator.sample_base_tile(d.tile).passable && d.family != DecorationFamily::Stone
            );
        }
    }
}

#[test]
fn a_loose_rock_cannot_spill_into_a_reserved_tile_across_a_chunk_edge() {
    let generator = fixture(512, 20, 20_000);
    let (anchor, edge) = (1..15)
        .flat_map(|y| (1..15).map(move |x| (x, y)))
        .find_map(|(x, y)| {
            candidate(&generator, x, y)
                .decorations
                .into_iter()
                .find(|d| {
                    d.family == DecorationFamily::Stone && d.tile.x % CHUNK_TILES == CHUNK_TILES - 1
                })
                .map(|d| (d.tile, TileCoord::new(d.tile.x + 1, d.tile.y)))
        })
        .expect("fixture has a pile on the east edge of a chunk");
    let next = candidate(&generator, edge.x / CHUNK_TILES, edge.y / CHUNK_TILES);
    let physical = generator
        .evaluate_physical_chunk_with_cancel(
            next.x,
            next.y,
            &policy,
            &|_| Reservations::default(),
            &|| false,
        )
        .unwrap()
        .unwrap();
    assert_eq!(physical.tiles, next.tiles);
    assert_eq!(physical.resources, next.resources);

    assert!(
        !next
            .tiles
            .iter()
            .find(|tile| tile.tile == edge)
            .unwrap()
            .terrain
            .passable
    );
    let reserved = |at| Reservations {
        route: at == edge,
        ..Reservations::default()
    };
    let point = |at| {
        generator
            .evaluate_landscape_point_with_cancel::<false>(at, &policy, &reserved, &|| false)
            .unwrap()
            .unwrap()
    };
    assert!(point(edge).tile.terrain.passable);
    assert!(
        point(anchor)
            .decoration
            .is_none_or(|d| d.family != DecorationFamily::Stone)
    );
}

#[test]
fn rock_coverage_visits_each_halo_anchor_once_and_stamps_overlap() {
    use std::{cell::RefCell, collections::BTreeSet};

    let origin = TileCoord::new(32, 32);
    let outside = TileCoord::new(31, 40);
    let inside = TileCoord::new(33, 40);
    let seen = RefCell::new(BTreeSet::new());
    let rocks = dressing::ChunkRocks::new(
        origin,
        96,
        |at| {
            assert!(seen.borrow_mut().insert((at.x, at.y)));
            Ok(at == outside || at == inside)
        },
        true,
    )
    .unwrap();
    assert_eq!(seen.into_inner().len(), 34 * 34);
    assert_eq!(rocks.at(0, 8), (false, true));
    assert_eq!(rocks.at(1, 8), (true, true));
    assert_eq!(rocks.at(2, 9), (false, true));
    assert_eq!(rocks.at(3, 9), (false, false));
}

#[test]
fn packed_and_decoded_navigation_block_the_full_rock_footprint() {
    let generator = fixture(96, 20, 20_000);
    let piles = (0..3).flat_map(|y| (0..3).map(move |x| (x, y)));
    let center = piles
        .flat_map(|(x, y)| {
            generator
                .landscape_chunk_with_cancel(x, y, &|| false)
                .unwrap()
                .decorations
        })
        .find(|decoration| {
            let interior = 1..CHUNK_TILES - 1;
            decoration.family == DecorationFamily::Stone
                && interior.contains(&(decoration.tile.x % CHUNK_TILES))
                && interior.contains(&(decoration.tile.y % CHUNK_TILES))
        })
        .expect("steppe fixture grows a rock wholly inside a chunk")
        .tile;
    let mut bytes = Vec::new();
    let mut records = Vec::new();
    for y in 0..3 {
        for x in 0..3 {
            let start = bytes.len();
            bytes.extend(
                generator
                    .prepare_static_navigation_chunk(x, y, &|| false)
                    .unwrap()
                    .encode()
                    .unwrap(),
            );
            records.push(start..bytes.len());
        }
    }
    let snapshot = Arc::new(
        crate::NavigationSnapshot::from_storage(3, Arc::new(bytes), records, &|| false).unwrap(),
    );
    let loaded = fixture(96, 20, 20_000);
    loaded
        .install_navigation_snapshot(snapshot.clone())
        .unwrap();
    let assert_footprint = || {
        for tile in dressing::rock_footprint(center) {
            let cell = loaded
                .walk_at_with_cancel(tile, &|| false)
                .unwrap()
                .unwrap();
            assert!(!cell.passable, "rock at {center:?} covers {tile:?}");
            assert!(cell.node.is_none());
        }
    };
    assert_footprint();
    let packed_bytes = snapshot.heap_bytes();
    for _ in 0..2 * CHUNK_TILES * CHUNK_TILES {
        assert!(
            !loaded
                .walk_at_with_cancel(center, &|| false)
                .unwrap()
                .unwrap()
                .passable
        );
    }
    assert!(
        snapshot.heap_bytes() > packed_bytes,
        "the chunk was decoded"
    );
    assert_footprint();
    assert_eq!(loaded.walk_cache_usage(), (0, 0));
}
