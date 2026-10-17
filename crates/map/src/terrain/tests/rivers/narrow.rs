//! A one-tile stream is real water: narrow, drawn as river, and still blocking.
use super::*;
use crate::RiverSegment;

fn stream(x0: i64, y0: i64, x1: i64, y1: i64) -> Vec<[i64; 7]> {
    let length = (x1 - x0).abs().max((y1 - y0).abs());
    vec![[x0 * 256, y0 * 256, x1 * 256, y1 * 256, 256, 0, length * 256]]
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

#[test]
fn a_one_tile_stream_still_blocks_a_straight_and_a_diagonal_crossing() {
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
        let route = crate::find_path(
            &generator,
            open_tile_near(&generator, from.0, from.1),
            open_tile_near(&generator, to.0, to.1),
            (WIDTH * WIDTH) as u32,
        );
        assert!(
            matches!(route, crate::MovementOutcome::Unreachable),
            "{name}: {route:?}"
        );
    }
}
