//! Crossings follow source stations, not evidence cells or chunk boundaries.
use super::*;

fn channel(width: i64) -> Vec<[i64; 7]> {
    vec![[130 * 256, -256, 130 * 256, 258 * 256, width, 0, 259 * 256]]
}

#[test]
fn medium_rivers_have_regular_visible_crossings_that_connect_both_banks() {
    for width in [3 * 256, 4 * 256, 6 * 256] {
        let generator = crossing_with_geometry(false, false, channel(width));
        // Each interior 40-tile period contains both a full-width ford and
        // blocking water. Check all wet tiles across the warped channel.
        for station in [40, 80, 120, 160, 200, 240] {
            let rows = (station - 4..=station + 2)
                .filter(|y| {
                    let wet = (120..140)
                        .map(|x| generator.tile_at(TileCoord::new(x, *y)).unwrap())
                        .filter(|t| t.water == WaterKind::River)
                        .collect::<Vec<_>>();
                    !wet.is_empty()
                        && wet
                            .iter()
                            .all(|t| t.material == GroundMaterial::Ford && t.passable)
                })
                .count();
            assert!(
                rows >= 2,
                "width {width}, station {station}: {rows} ford rows"
            );
            let between = (120..140)
                .map(|x| generator.tile_at(TileCoord::new(x, station - 21)).unwrap())
                .filter(|t| t.water == WaterKind::River)
                .collect::<Vec<_>>();
            assert!(!between.is_empty(), "the warped channel remains continuous");
            assert!(
                between
                    .iter()
                    .all(|t| !t.passable && t.material != GroundMaterial::Ford),
                "blocking water between crossings"
            );
        }
        let bank = |xs: std::ops::Range<i32>| {
            xs.map(|x| TileCoord::new(x, 100))
                .find(|at| {
                    generator.tile_at(*at).unwrap().passable && generator.object_at(*at).is_none()
                })
                .expect("open bank beside the crossing")
        };
        let from = bank(115..121);
        let to = bank(141..147);
        let route = crate::find_path(&generator, from, to, (WIDTH * WIDTH) as u32);
        let crate::MovementOutcome::Path(path) = route else {
            panic!("width {width}: {route:?}");
        };
        assert!(
            path.tiles
                .iter()
                .any(|at| { generator.tile_at(*at).unwrap().material == GroundMaterial::Ford }),
            "the bank-to-bank path actually wades through water"
        );
    }
}

#[test]
fn a_diagonal_medium_river_routes_land_units_through_a_ford() {
    let generator = crossing_with_geometry(
        false,
        false,
        vec![[-256, -256, 258 * 256, 258 * 256, 4 * 256, 0, 367 * 256]],
    );
    let route = crate::find_path(
        &generator,
        TileCoord::new(60, 200),
        TileCoord::new(200, 60),
        (WIDTH * WIDTH) as u32,
    );
    let crate::MovementOutcome::Path(path) = route else {
        panic!("{route:?}");
    };
    assert!(
        path.tiles
            .iter()
            .any(|at| generator.tile_at(*at).unwrap().material == GroundMaterial::Ford)
    );
}

#[test]
fn deep_main_channel_blocks_even_where_a_tributary_has_a_ford() {
    let mut segments = channel(7 * 256);
    segments.push([
        60 * 256,
        119 * 256,
        160 * 256,
        119 * 256,
        4 * 256,
        0,
        100 * 256,
    ]);
    let generator = crossing_with_geometry(false, false, segments);
    for y in 1..WIDTH - 1 {
        let tile = generator.tile_at(TileCoord::new(130, y)).unwrap();
        assert_eq!(tile.water, WaterKind::River);
        assert_ne!(tile.material, GroundMaterial::Ford, "main channel {y}");
        assert!(!tile.passable, "main channel {y}");
    }
    assert_eq!(
        generator
            .tile_at(TileCoord::new(100, 119))
            .unwrap()
            .material,
        GroundMaterial::Ford,
        "tributary station 40 remains a crossing away from the main river"
    );
}

#[test]
fn ocean_never_inherits_river_fords() {
    let generator = crossing_with_geometry(true, false, channel(4 * 256));
    let tiles = composed(&generator);
    let sea = tiles
        .iter()
        .filter(|p| p.terrain.water == WaterKind::Ocean)
        .collect::<Vec<_>>();
    assert!(!sea.is_empty());
    for tile in sea {
        assert!(!tile.terrain.passable);
        assert_ne!(tile.terrain.material, GroundMaterial::Ford);
    }
}

#[test]
fn ford_phase_survives_closely_segmented_meander() {
    // Vertices about one tile apart wiggle the six-tile river around its
    // station-40 ford. Neighbouring pieces outside the ford band must not
    // veto the crossing: the phase belongs to the river, not to a piece.
    let mut segments = vec![[130 * 256, -256, 130 * 256, 20 * 256, 6 * 256, 0, 21 * 256]];
    let wiggle = |y: i64| 130 * 256 + if y % 2 == 0 { 154 } else { -154 };
    for y in 20..60 {
        segments.push([
            if y == 20 { 130 * 256 } else { wiggle(y) },
            y * 256,
            wiggle(y + 1),
            (y + 1) * 256,
            6 * 256,
            (y + 1) * 256,
            (y + 2) * 256,
        ]);
    }
    segments.push([
        wiggle(60),
        60 * 256,
        130 * 256,
        258 * 256,
        6 * 256,
        61 * 256,
        259 * 256,
    ]);
    let generator = crossing_with_geometry(false, false, segments);
    let ford_rows = (37..=40)
        .filter(|y| {
            let wet = (120..140)
                .map(|x| generator.tile_at(TileCoord::new(x, *y)).unwrap())
                .filter(|t| t.water == WaterKind::River)
                .collect::<Vec<_>>();
            !wet.is_empty()
                && wet
                    .iter()
                    .all(|t| t.material == GroundMaterial::Ford && t.passable)
        })
        .count();
    assert!(ford_rows >= 2, "{ford_rows} walkable ford rows");
    let bank = |xs: std::ops::Range<i32>| {
        xs.map(|x| TileCoord::new(x, 100))
            .find(|at| {
                generator.tile_at(*at).unwrap().passable && generator.object_at(*at).is_none()
            })
            .expect("open bank beside the crossing")
    };
    let route = crate::find_path(
        &generator,
        bank(115..121),
        bank(141..147),
        (WIDTH * WIDTH) as u32,
    );
    let crate::MovementOutcome::Path(path) = route else {
        panic!("{route:?}");
    };
    assert!(
        path.tiles
            .iter()
            .any(|at| generator.tile_at(*at).unwrap().material == GroundMaterial::Ford)
    );
}
