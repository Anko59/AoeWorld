//! A chunk near a gate keeps its bulk capture and classifies only the gate
//! tiles and their neighbours one by one: the result equals classifying
//! every tile one by one (#1051).
use super::*;

/// Offers a bulk capture computed as the terrain sees the ground (a gate
/// is a building: blocked), and the gates as special tiles.
struct Bulk<'a> {
    grid: &'a Grid,
    terrain: Grid,
    gates: Vec<TileCoord>,
}

impl RegionGround for Bulk<'_> {
    fn chunk(&self, key: (i32, i32), size: (i32, i32)) -> Option<RegionChunk> {
        Some(label::classified(&self.terrain, key, size))
    }
    fn special_tiles(&self, min: TileCoord, max: TileCoord) -> Vec<TileCoord> {
        self.gates
            .iter()
            .copied()
            .filter(|t| t.x >= min.x && t.y >= min.y && t.x <= max.x && t.y <= max.y)
            .collect()
    }
    fn tile(&self, tile: TileCoord) -> RegionTile {
        self.grid.tile(tile)
    }
    fn edge_clear(&self, from: TileCoord, to: TileCoord) -> bool {
        self.grid.edge_clear(from, to)
    }
    fn diagonal_override(&self, from: TileCoord, to: TileCoord) -> Option<bool> {
        self.grid.diagonal_override(from, to)
    }
    fn gate_owner(&self, gate: u32) -> u32 {
        self.grid.gate_owner(gate)
    }
}

#[test]
fn a_chunk_near_a_gate_patches_its_bulk_capture_to_the_per_tile_result() {
    let size = (96, 96);
    // Inside chunk (1, 1), on its west border, and just outside it in the
    // halo (the last column of chunk (0, 1)).
    let gates = [
        TileCoord::new(45, 40),
        TileCoord::new(32, 50),
        TileCoord::new(31, 60),
    ];
    let mut grid = Grid::new(size.0, size.1);
    let mut terrain = Grid::new(size.0, size.1);
    for (n, &tile) in gates.iter().enumerate() {
        let id = n as u32 + 1;
        grid.set(tile, RegionTile::Gate(id));
        grid.owners.insert(id, 1);
        terrain.set(tile, RegionTile::Blocked);
    }
    // A wall beside one gate, so edges as well as classes matter.
    for y in 38..43 {
        grid.set(TileCoord::new(47, y), RegionTile::Blocked);
        terrain.set(TileCoord::new(47, y), RegionTile::Blocked);
    }
    let ground = Bulk {
        grid: &grid,
        terrain,
        gates: gates.to_vec(),
    };
    for key in [(1, 1), (0, 1)] {
        let patched = label::input(&ground, key, size);
        let full = label::classified(&grid, key, size);
        assert_eq!(patched.classes, full.classes, "classes of chunk {key:?}");
        assert_eq!(patched.signature, full.signature, "edges of chunk {key:?}");
    }
    // The bulk capture alone differs: the test exercises the patch.
    assert_ne!(
        ground.chunk((1, 1), size).unwrap().signature,
        label::classified(&grid, (1, 1), size).signature
    );
}
