use super::*;

#[test]
fn edge_restricted_cell_scan_equals_the_full_neighborhood_scan() {
    let full = |key, domain: &[u8], tile: TileCoord, columns: i32, rows: i32| {
        let (cx, cy) = (tile.x.div_euclid(columns), tile.y.div_euclid(rows));
        for y in cy - 1..=cy + 1 {
            for x in cx - 1..=cx + 1 {
                let (root, value, count) = patch_origin(key, domain, x, y, columns, rows);
                if let Some(slot) = patch_slot(tile, root, count) {
                    return Some((slot, value));
                }
            }
        }
        None
    };
    for key in [[7; 32], [19; 32]] {
        for (domain, columns, rows) in [
            (DOMAINS[0], FORAGE_COLUMNS, FORAGE_ROWS),
            (DOMAINS[1], GOLD_COLUMNS, GOLD_ROWS),
            (DOMAINS[2], STONE_COLUMNS, STONE_ROWS),
        ] {
            let mut hits = 0;
            for y in -200..200 {
                for x in -200..200 {
                    let tile = TileCoord::new(x, y);
                    let expected = full(key, domain, tile, columns, rows);
                    hits += usize::from(expected.is_some());
                    assert_eq!(patch_node(key, domain, tile, columns, rows), expected);
                }
            }
            assert!(hits > 0);
        }
    }
}

#[test]
fn patches_are_bounded_and_identical_from_every_query_direction() {
    let key = [7; 32];
    let nodes = (-256..256)
        .flat_map(|y| {
            (-256..256).filter_map(move |x| {
                patch_node(
                    key,
                    b"stone-patch-v2",
                    TileCoord::new(x, y),
                    STONE_COLUMNS,
                    STONE_ROWS,
                )
                .map(|(slot, _)| (x, y, slot))
            })
        })
        .collect::<Vec<_>>();
    assert!(nodes.windows(2).all(|pair| pair[0] != pair[1]));
    assert!(nodes.iter().all(|(_, _, slot)| *slot < 8));
}

#[test]
fn forage_patch_candidates_are_always_between_four_and_eight() {
    let counts = (0u64..256)
        .map(|value| patch_count(value.rotate_left(29)))
        .collect::<Vec<_>>();
    assert_eq!(counts.iter().copied().min(), Some(4));
    assert_eq!(counts.iter().copied().max(), Some(8));
}

#[test]
fn gold_and_stone_use_independent_streams_with_distinct_spacing() {
    let key = [11; 32];
    let gold_domain = b"gold-patch-v2";
    let stone_domain = b"stone-patch-v2";
    assert_ne!(
        super::super::unsigned_noise(key, gold_domain, 3, 5),
        super::super::unsigned_noise(key, stone_domain, 3, 5)
    );
    let (gold_root, _, _) = patch_origin(key, gold_domain, 3, 5, GOLD_COLUMNS, GOLD_ROWS);
    let (stone_root, _, _) = patch_origin(key, stone_domain, 3, 5, STONE_COLUMNS, STONE_ROWS);
    assert_ne!(gold_root, stone_root);
}

#[test]
fn ore_clusters_are_scarce_even_on_fully_suitable_land() {
    let key = [7; 32];
    for (domain, columns, rows, maximum) in [
        (DOMAINS[1], GOLD_COLUMNS, GOLD_ROWS, 72),
        (DOMAINS[2], STONE_COLUMNS, STONE_ROWS, 144),
    ] {
        let count = (0..512)
            .flat_map(|y| (0..512).map(move |x| TileCoord::new(x, y)))
            .filter(|tile| patch_node(key, domain, *tile, columns, rows).is_some())
            .count();
        assert!(
            count > 0 && count <= maximum,
            "{count} ore tiles on 512² suitable land"
        );
    }
}
