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
