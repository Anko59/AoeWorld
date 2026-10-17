//! A one-tile stream is real water: narrow, drawn as river, and still blocking.
use super::*;
use crate::RiverSegment;

fn stream(x0: i64, y0: i64, x1: i64, y1: i64) -> Vec<[i64; 7]> {
    channel(x0, y0, x1, y1, 256)
}

fn channel(x0: i64, y0: i64, x1: i64, y1: i64, width: i64) -> Vec<[i64; 7]> {
    let length = (x1 - x0).abs().max((y1 - y0).abs());
    vec![[
        x0 * 256,
        y0 * 256,
        x1 * 256,
        y1 * 256,
        width,
        0,
        length * 256,
    ]]
}

#[test]
fn a_one_tile_segment_is_a_valid_river_page_entry() {
    let segment = RiverSegment {
        start: [0, 0],
        end: [0, 64 * 256],
        width_units: 256,
        station_start: 0,
        station_end: 64 * 256,
        source_delta: [0, 64 * 256],
        source_station_delta: 64 * 256,
    };
    segment.validate().expect("a one-tile stream is allowed");
    let wide = RiverSegment {
        width_units: 10 * 256,
        ..segment
    };
    wide.validate().expect("ten tiles stay allowed");
    let wider = RiverSegment {
        width_units: 10 * 256 + 1,
        ..segment
    };
    assert!(wider.validate().is_err());
}

#[test]
fn a_one_tile_stream_is_at_most_two_tiles_wide_on_every_row() {
    let generator = crossing_with_geometry(false, false, stream(130, -1, 130, 258));
    let tiles = composed(&generator);
    let mut wet_rows = 0;
    for y in 8..WIDTH - 8 {
        let wet = (0..WIDTH)
            .filter(|x| at(&tiles, *x, y).unwrap().terrain.water == WaterKind::River)
            .count();
        assert!(wet <= 2, "row {y} holds {wet} river tiles");
        wet_rows += usize::from(wet > 0);
    }
    assert!(wet_rows > 200, "the stream is continuous: {wet_rows} rows");
}

fn open_tile_near(generator: &MapChunkGenerator, x: i32, y: i32) -> TileCoord {
    (-6..=6)
        .flat_map(|dy| (-6..=6).map(move |dx| TileCoord::new(x + dx, y + dy)))
        .find(|t| {
            generator
                .tile_at(*t)
                .is_some_and(|tile| tile.passable && tile.water == WaterKind::None)
                && generator.object_at(*t).is_none()
        })
        .expect("open land beside the stream")
}

fn crossing_route(
    generator: &MapChunkGenerator,
    from: (i32, i32),
    to: (i32, i32),
) -> crate::MovementOutcome {
    crate::find_path(
        generator,
        open_tile_near(generator, from.0, from.1),
        open_tile_near(generator, to.0, to.1),
        (WIDTH * WIDTH) as u32,
    )
}

#[test]
fn a_one_tile_stream_is_wadeable_shallows_that_units_path_through() {
    for (name, segments, from, to) in [
        (
            "straight",
            stream(130, -1, 130, 258),
            (100, 128),
            (160, 128),
        ),
        ("diagonal", stream(-1, -1, 258, 258), (60, 200), (200, 60)),
    ] {
        let generator = crossing_with_geometry(false, false, segments);
        let tiles = composed(&generator);
        let wet = tiles
            .iter()
            .filter(|p| p.terrain.water == WaterKind::River)
            .collect::<Vec<_>>();
        assert!(!wet.is_empty(), "{name}");
        for point in &wet {
            assert_eq!(point.terrain.material, GroundMaterial::Ford, "{name}");
            assert!(point.terrain.passable, "{name} {:?}", point.tile);
        }
        let route = crossing_route(&generator, from, to);
        assert!(
            matches!(route, crate::MovementOutcome::Path(_)),
            "{name}: {route:?}"
        );
    }
}

#[test]
fn a_river_wider_than_the_shallows_limit_still_blocks_straight_and_diagonal() {
    for (name, segments, from, to) in [
        (
            "straight",
            channel(130, -1, 130, 258, 4 * 256),
            (100, 128),
            (160, 128),
        ),
        (
            "diagonal",
            channel(-1, -1, 258, 258, 4 * 256),
            (60, 200),
            (200, 60),
        ),
    ] {
        let generator = crossing_with_geometry(false, false, segments);
        for point in composed(&generator)
            .iter()
            .filter(|p| p.terrain.water == WaterKind::River)
        {
            assert_ne!(point.terrain.material, GroundMaterial::Ford, "{name}");
            assert!(!point.terrain.passable, "{name} {:?}", point.tile);
        }
        let route = crossing_route(&generator, from, to);
        assert!(
            matches!(route, crate::MovementOutcome::Unreachable),
            "{name}: {route:?}"
        );
    }
}

#[test]
fn a_stream_joining_a_wide_river_stops_being_shallows_inside_the_wide_channel() {
    let mut segments = channel(130, -1, 130, 258, 5 * 256);
    segments.extend(channel(60, 128, 130, 128, 256));
    let generator = crossing_with_geometry(false, false, segments);
    let tiles = composed(&generator);
    let tile = |x, y| at(&tiles, x, y).unwrap();
    assert_eq!(tile(80, 128).terrain.material, GroundMaterial::Ford);
    assert!(tile(80, 128).terrain.passable);
    for y in [60, 128, 200] {
        assert_eq!(tile(130, y).terrain.water, WaterKind::River);
        assert_ne!(
            tile(130, y).terrain.material,
            GroundMaterial::Ford,
            "row {y}"
        );
        assert!(!tile(130, y).terrain.passable, "row {y}");
    }
}

#[test]
fn a_narrow_stream_on_sloping_terrain_is_ford_on_every_tile() {
    // Ratio one: 240 cm over eight tiles is about the traversable grade, so
    // the land under and beside the stream is steep enough to be cliff-graded.
    let generator = crossing_with_height(
        false,
        Some(&|x, y| (32 - x) * 240 + (y % 7) * 60),
        stream(130, -1, 130, 258),
    );
    let tiles = composed(&generator);
    let mut river = 0;
    for point in tiles.iter().filter(|p| p.terrain.water == WaterKind::River) {
        river += 1;
        assert_eq!(
            point.terrain.material,
            GroundMaterial::Ford,
            "{:?}",
            point.tile
        );
        assert!(point.terrain.passable, "{:?}", point.tile);
    }
    assert!(river > 250, "the stream is long: {river} tiles");
}

#[test]
fn a_stream_up_to_two_and_a_half_tiles_is_one_continuous_ford() {
    // 2.5 tiles is 1 m3/s: the streams above it are rivers. Without a single
    // width limit a stream would alternate between wading and blocking stretches.
    for (name, width) in [
        ("one and a half", 384),
        ("two", 512),
        ("two and a half", 640),
    ] {
        let generator = crossing_with_geometry(false, false, channel(130, -1, 130, 258, width));
        let tiles = composed(&generator);
        let river = tiles
            .iter()
            .filter(|p| p.terrain.water == WaterKind::River)
            .collect::<Vec<_>>();
        assert!(river.len() > 250, "{name}");
        for point in river {
            assert_eq!(
                point.terrain.material,
                GroundMaterial::Ford,
                "{name} {:?}",
                point.tile
            );
            assert!(point.terrain.passable, "{name} {:?}", point.tile);
        }
    }
}
