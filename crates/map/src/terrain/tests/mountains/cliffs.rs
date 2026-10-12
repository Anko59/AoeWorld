use super::*;
use crate::CompactChunk;

/// Maximal runs of `true` as (start, length).
fn runs(values: &[bool]) -> Vec<(usize, usize)> {
    let mut result = Vec::new();
    let mut start = None;
    for (index, value) in values.iter().copied().chain([false]).enumerate() {
        match (value, start) {
            (true, None) => start = Some(index),
            (false, Some(first)) => {
                result.push((first, index - first));
                start = None;
            }
            _ => {}
        }
    }
    result
}

#[test]
fn a_steep_slope_gets_cliff_lines_along_its_contours_broken_by_passes() {
    // 15% grade rising to the west: contours run north-south and the slope
    // faces the camera.
    let tiles = base_tiles(&plane(-432, 0));
    // Rows clear of the start glade and its connector.
    let column = |x: i32| {
        (600..744)
            .map(|y| is_cliff(tiles[(y * SIDE + x) as usize]))
            .collect::<Vec<_>>()
    };
    let lines = runs(
        &(96..672)
            .map(|x| column(x).contains(&true))
            .collect::<Vec<_>>(),
    );
    assert!(lines.len() >= 8, "cliff lines: {lines:?}");
    for pair in lines.windows(2) {
        assert!(pair[1].0 - pair[0].0 >= 12, "lines too close: {lines:?}");
    }
    let (mut blocked, mut total) = (0, 0);
    for &(start, width) in &lines {
        assert_eq!(width, 3, "a cliff line is one three-tile piece thick");
        let line = column(96 + start as i32);
        // The whole thickness is blocked or open together along a contour.
        for offset in 1..3 {
            assert!(line == column(96 + (start + offset) as i32));
        }
        blocked += line.iter().filter(|cliff| **cliff).count();
        total += line.len();
        for (_, length) in runs(&line) {
            assert_eq!(length % 3, 0, "pieces are whole");
        }
        for (_, length) in runs(&line.iter().map(|cliff| !cliff).collect::<Vec<_>>()) {
            assert!(length >= 3, "a pass is at least one piece wide");
        }
    }
    let share = blocked * 100 / total;
    eprintln!(
        "lines={} blocked share along contours={share}%",
        lines.len()
    );
    assert!(
        (35..=85).contains(&share),
        "passes keep lines open: {share}%"
    );
}

#[test]
fn gentle_flat_and_away_facing_ground_has_no_cliffs() {
    for generator in [
        plane(0, 0),
        plane(-110, 0),
        plane(-80, -80),
        plane(432, 0),
        plane(300, 300),
    ] {
        assert!(!base_tiles(&generator).into_iter().any(is_cliff));
    }
}

#[test]
fn cliffs_are_relief_not_seeded_objects_and_spare_the_start() {
    let (first, second) = (alpine(1), alpine(977));
    let tiles = base_tiles(&first);
    let mut cliffs = 0;
    for (index, tile) in tiles.iter().enumerate() {
        let position = TileCoord::new(index as i32 % SIDE, index as i32 / SIDE);
        cliffs += usize::from(is_cliff(*tile));
        // Only the seeded start connector may differ between seeds.
        let near_start = (position.x - SIDE / 2).abs() < 224 && (position.y - SIDE / 2).abs() < 224;
        if index % 7 == 0 && !near_start {
            assert!(*tile == second.sample_base_tile(position));
        }
        let reserved = first.landscape_reservations_at(position);
        assert!(!((reserved.start || reserved.route) && is_cliff(*tile)));
    }
    let share = cliffs * 1000 / tiles.len();
    eprintln!("alpine cliff tiles={cliffs} ({share} per thousand)");
    assert!((10..=120).contains(&share), "cliff share {share}‰");
}

#[test]
fn mountains_stay_one_reachable_country_with_edge_to_edge_crossings() {
    for (name, generator) in [
        ("alpine", alpine(1)),
        (
            "cone",
            relief(1, 13, |x, y| 3_400 - 420 * (x - 8).abs().max((y - 8).abs())),
        ),
        (
            "pit",
            relief(1, 13, |x, y| 400 + 420 * (x - 8).abs().max((y - 8).abs())),
        ),
    ] {
        let tiles = base_tiles(&generator);
        let open = tiles.iter().map(|tile| tile.passable).collect::<Vec<_>>();
        let passable = open.iter().filter(|open| **open).count();
        let (sizes, edges) = regions(&open);
        eprintln!(
            "{name}: passable={passable} of {} regions={} largest={} second={:?} edges={edges:?}",
            open.len(),
            sizes.len(),
            sizes[0],
            sizes.get(1)
        );
        assert!(
            tiles.iter().any(|tile| is_cliff(*tile)),
            "{name} has cliffs"
        );
        assert_eq!(edges, [true; 4], "{name}: crossings reach all four edges");
        // Nothing larger than a pocket between two pieces is cut off.
        assert!(sizes[0] * 1000 >= passable * 999, "{name}: {sizes:?}");
        assert!(
            sizes.get(1).is_none_or(|size| *size <= 36),
            "{name}: {sizes:?}"
        );
    }
}

#[test]
fn every_cliff_piece_is_one_sprite_on_an_impassable_cell() {
    let generator = alpine(1);
    let (mut pieces, mut facings) = (0, [0_usize; 8]);
    for chunk_y in 4..12 {
        for chunk_x in 4..12 {
            let chunk = generator
                .landscape_chunk_with_cancel(chunk_x, chunk_y, &|| false)
                .expect("chunk");
            let decoded = CompactChunk::encode(&chunk)
                .and_then(|encoded| encoded.decode())
                .expect("codec");
            assert_eq!(decoded, chunk);
            let tile = |position: TileCoord| {
                chunk
                    .tiles
                    .iter()
                    .find(|tile| tile.tile == position)
                    .map(|tile| tile.terrain)
            };
            for decoration in chunk.decorations.iter().filter(|d| d.family as u8 == 4) {
                pieces += 1;
                facings[usize::from(decoration.orientation)] += 1;
                assert_eq!((decoration.tile.x % 3, decoration.tile.y % 3), (1, 1));
                assert!(is_cliff(tile(decoration.tile).expect("anchor tile")));
            }
            for center in chunk.tiles.iter().filter(|tile| {
                tile.tile.x % 3 == 1 && tile.tile.y % 3 == 1 && is_cliff(tile.terrain)
            }) {
                let sprites = chunk
                    .decorations
                    .iter()
                    .filter(|d| d.tile == center.tile && d.family as u8 == 4)
                    .count();
                let reserved = generator.landscape_reservations_at(center.tile);
                assert_eq!(sprites, usize::from(!reserved.start && !reserved.route));
                assert!(!chunk.resources.iter().any(|r| r.node.tile == center.tile));
            }
        }
    }
    eprintln!("pieces={pieces} by facing={facings:?}");
    assert!(pieces > 100);
    // Slopes falling away from the camera stay open.
    assert_eq!(facings[3] + facings[4] + facings[5], 0);
    assert!(facings.iter().filter(|count| **count > 0).count() >= 3);
}

#[test]
fn cliff_cells_below_the_snow_line_take_summer_ground_not_base_snow() {
    // A boreal hollow: cliffs between 300 m and 2,000 m.
    let boreal = relief(1, 15, |x, y| 300 + 420 * (x - 8).abs().max((y - 8).abs()));
    let mut cliffs = 0;
    for y in (300..468).step_by(2) {
        for x in (0..SIDE).step_by(2) {
            let tile = boreal
                .landscape_point_with_cancel(TileCoord::new(x, y), &|| false)
                .expect("point")
                .expect("in bounds")
                .tile
                .terrain;
            if is_cliff(tile) && tile.geographic_height_centimeters < 200_000 {
                cliffs += 1;
                assert_ne!(tile.material, GroundMaterial::Snow);
            }
        }
    }
    assert!(cliffs > 50, "fixture has low cliffs: {cliffs}");
}
