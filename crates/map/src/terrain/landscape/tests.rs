use super::*;
use crate::MapChunkGenerator;
use aoe_core::TileCoord;
use std::collections::BTreeSet;

mod connected;

fn generator() -> MapChunkGenerator {
    MapChunkGenerator::new([17; 32], 5, 1_024)
}

#[test]
fn trail_nodes_are_seeded_jittered_and_confined_to_their_cells() {
    let terrain = generator();
    let mut offsets = BTreeSet::new();
    for cell_y in -2..12 {
        for cell_x in -2..12 {
            let center = node_center(&terrain, (cell_x, cell_y));
            let regular = TileCoord::new(
                cell_x * OPENING_GRID_TILES + OPENING_GRID_TILES / 2,
                cell_y * OPENING_GRID_TILES + OPENING_GRID_TILES / 2,
            );
            assert!(center.x.abs_diff(regular.x) <= 40 && center.y.abs_diff(regular.y) <= 40);
            let tile = terrain
                .forest_opening_center_at(regular)
                .expect("node tile");
            assert!(tile.x.abs_diff(center.x) <= 16 && tile.y.abs_diff(center.y) <= 16);
            offsets.insert((center.x - regular.x, center.y - regular.y));
        }
    }
    assert!(
        offsets.len() > 150,
        "nodes must not sit on a regular lattice"
    );
    assert_ne!(
        node_center(&terrain, (3, 3)),
        node_center(&MapChunkGenerator::new([18; 32], 5, 1_024), (3, 3))
    );
}

#[test]
fn only_the_starting_glade_and_its_connector_are_reserved() {
    let terrain = generator();
    let center = TileCoord::new(511, 511);
    let reservations = terrain.landscape_reservations_at(center);
    assert!(reservations.start && reservations.route);
    assert!(!starting_glade(&terrain, TileCoord::new(511 + 28, 511)));
    let (mut glade, mut routes) = (0, 0);
    for y in 0..1_024 {
        for x in 0..1_024 {
            let reservations = terrain.landscape_reservations_at(TileCoord::new(x, y));
            glade += usize::from(reservations.start);
            routes += usize::from(reservations.route);
            assert!(!reservations.resource_approach);
        }
    }
    // One glade of radius 18..=26 and one connector about five tiles wide:
    // no opening lattice and no trail network is reserved.
    assert!((900..2_300).contains(&glade), "glade={glade}");
    assert!((5..1_400).contains(&routes), "routes={routes}");
    let far = terrain.landscape_reservations_at(TileCoord::new(-500, -500));
    assert!(!far.route && !far.start);
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
