use super::*;
use crate::MapChunkGenerator;
use aoe_core::TileCoord;
use std::collections::BTreeSet;

mod connected;

fn generator() -> MapChunkGenerator {
    MapChunkGenerator::new([17; 32], 5, 1_024)
}

#[test]
fn openings_are_seeded_irregular_variable_and_confined_to_their_cells() {
    let terrain = generator();
    let mut active = 0;
    let mut widths = BTreeSet::new();
    let mut heights = BTreeSet::new();
    let mut jittered = false;
    for cell_y in 0..12 {
        for cell_x in 0..12 {
            let cell = (cell_x, cell_y);
            let shape = opening_geometry_for_cell(&terrain, cell);
            active += 1;
            let bounds = shape.vertices.iter().fold(
                (i32::MAX, i32::MAX, i32::MIN, i32::MIN),
                |bounds, vertex| {
                    (
                        bounds.0.min(vertex.x),
                        bounds.1.min(vertex.y),
                        bounds.2.max(vertex.x),
                        bounds.3.max(vertex.y),
                    )
                },
            );
            assert!(bounds.0 >= cell_x * OPENING_GRID_TILES);
            assert!(bounds.1 >= cell_y * OPENING_GRID_TILES);
            assert!(bounds.2 < (cell_x + 1) * OPENING_GRID_TILES);
            assert!(bounds.3 < (cell_y + 1) * OPENING_GRID_TILES);
            widths.insert(bounds.2 - bounds.0);
            heights.insert(bounds.3 - bounds.1);
            let regular_center = TileCoord::new(
                cell_x * OPENING_GRID_TILES + OPENING_GRID_TILES / 2,
                cell_y * OPENING_GRID_TILES + OPENING_GRID_TILES / 2,
            );
            let center = shape_center(&terrain, cell);
            jittered |= center != regular_center;
            assert!(opening_contains(&terrain, center));
        }
    }
    assert_eq!(active, 144, "every lattice cell has an opening node");
    assert!(widths.len() >= 12, "opening widths: {widths:?}");
    assert!(heights.len() >= 12, "opening heights: {heights:?}");
    assert!(jittered);
    assert!(!opening_contains(&terrain, TileCoord::new(-1, -1)));
}

fn shape_center(generator: &MapChunkGenerator, cell: (i32, i32)) -> TileCoord {
    opening_center(cell_layout(generator, cell), cell)
}

#[test]
fn procedural_paths_bend_between_openings_and_reserve_routes() {
    let terrain = generator();
    let mut links = 0;
    for cell_y in -2..12 {
        for cell_x in -2..12 {
            for axis in 0..=1 {
                let Some(segment) = trail_segment(&terrain, (cell_x, cell_y), axis) else {
                    continue;
                };
                links += 1;
                assert!(opening_contains(&terrain, segment.start));
                assert!(opening_contains(&terrain, segment.end));
                for point in [segment.start, segment.bend, segment.end] {
                    assert!(procedural_trail_contains(&terrain, point));
                    assert!(terrain.landscape_reservations_at(point).route);
                }
                assert!(near_segment(
                    segment.start,
                    segment.bend,
                    segment.start,
                    segment.half_width
                ));
                assert!(near_segment(
                    segment.bend,
                    segment.end,
                    segment.end,
                    segment.half_width
                ));
            }
        }
    }
    assert_eq!(links, 2 * 14 * 14, "every lattice edge is a trail");
    let reservations = terrain.landscape_reservations_at(TileCoord::new(-500, -500));
    assert!(!reservations.route && !reservations.start);
}

#[test]
fn chunks_repeat_across_a_chunk_edge() {
    let first = generator();
    let repeated = generator();
    for chunk_y in [2, 3] {
        let left = first.chunk(0, chunk_y).expect("left chunk");
        let right = first.chunk(1, chunk_y).expect("right chunk");
        assert_eq!(left, repeated.chunk(0, chunk_y).expect("repeat left"));
        assert_eq!(right, repeated.chunk(1, chunk_y).expect("repeat right"));
        for offset in 0..32 {
            let coord = TileCoord::new(31, chunk_y * 32 + offset);
            assert_eq!(
                first.tile_at(coord),
                repeated.tile_at(coord),
                "west side of seam: {coord:?}"
            );
            let coord = TileCoord::new(32, chunk_y * 32 + offset);
            assert_eq!(
                first.tile_at(coord),
                repeated.tile_at(coord),
                "east side of seam: {coord:?}"
            );
        }
    }
}
