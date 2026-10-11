//! Shared procedural reservations around the start: the central starting
//! glade and one connector to the trail node of its cell. The glade always
//! clears vegetation; the connector is a corridor that the forest descriptor
//! clears only where it crosses a large forest, joining a start inside one to
//! that forest's hub and trails. Trail nodes and
//! the trails between them belong to `landscape_patches`. Source water and
//! cliffs still decide actual traversal.
use super::{MapChunkGenerator, geometry};
use crate::landscape_patches::{CELL_TILES, Patches};
use aoe_core::TileCoord;

const OPENING_GRID_TILES: i32 = CELL_TILES as i32;

impl MapChunkGenerator {
    /// Pure, bounded lookup of the trail-node tile of this tile's 192-tile
    /// cell (the cell is taken in tile space; the node itself lives in the
    /// forest descriptor's warped space). A node is not a claim that source water or cliffs are traversable, and
    /// its hub glade exists only inside a large forest.
    pub fn forest_opening_center_at(&self, tile: TileCoord) -> Option<TileCoord> {
        let (x, y) = Patches::new(self.geography_key, self.procedural_seed).node_tile(
            i64::from(tile.x.div_euclid(OPENING_GRID_TILES)),
            i64::from(tile.y.div_euclid(OPENING_GRID_TILES)),
        );
        Some(TileCoord::new(clamp_tile(x), clamp_tile(y)))
    }
}

fn node_center(generator: &MapChunkGenerator, cell: (i32, i32)) -> TileCoord {
    let (x, y) = Patches::new(generator.geography_key, generator.procedural_seed)
        .node(i64::from(cell.0), i64::from(cell.1));
    TileCoord::new(clamp_tile(x), clamp_tile(y))
}

fn clamp_tile(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

pub(super) fn starting_glade(generator: &MapChunkGenerator, tile: TileCoord) -> bool {
    let width = generator.width_tiles.max(1);
    let center = TileCoord::new((width - 1) / 2, (width - 1) / 2);
    if tile.x.abs_diff(center.x) > 27 || tile.y.abs_diff(center.y) > 27 {
        return false;
    }
    let layout = geometry::cell_value(generator, b"forest-starting-glade-v1", (width, 0), 0);
    let base_radius = 20 + ((layout >> 12) % 5) as i32;
    let x_scale = 88 + ((mix64(layout ^ 0x2a5d_3c91) >> 11) % 13) as i32;
    let y_scale = 88 + ((mix64(layout ^ 0x7c4a_1e63) >> 9) % 13) as i32;
    let vertices: [TileCoord; 16] = std::array::from_fn(|vertex| {
        let (direction_x, direction_y) = geometry::DIRECTIONS[vertex];
        let jitter = ((mix64(layout ^ vertex as u64) >> 17) % 5) as i32 - 2;
        let radius = base_radius + jitter;
        let offset_x = i64::from(direction_x) * i64::from(radius) * i64::from(x_scale)
            / (geometry::DIRECTION_SCALE * 100);
        let offset_y = i64::from(direction_y) * i64::from(radius) * i64::from(y_scale)
            / (geometry::DIRECTION_SCALE * 100);
        TileCoord::new(
            center
                .x
                .saturating_add(i32::try_from(offset_x).unwrap_or(0)),
            center
                .y
                .saturating_add(i32::try_from(offset_y).unwrap_or(0)),
        )
    });
    geometry::point_in_polygon(&vertices, tile)
}

pub(super) fn starting_connector_contains(generator: &MapChunkGenerator, tile: TileCoord) -> bool {
    let coordinate = (generator.width_tiles.max(1) - 1) / 2;
    let center = TileCoord::new(coordinate, coordinate);
    // Reject far-away tiles before hashing; no map-wide allocation or source
    // search during sampling.
    let reach = OPENING_GRID_TILES + 16;
    if tile.x.abs_diff(center.x) > reach as u32 || tile.y.abs_diff(center.y) > reach as u32 {
        return false;
    }
    // The connector lives in the descriptor's warped space, like its
    // trails, so it ends exactly on the node's hub glade.
    let patches = Patches::new(generator.geography_key, generator.procedural_seed);
    let warped = |tile: TileCoord| {
        let (x, y) = patches.warped(i64::from(tile.x), i64::from(tile.y));
        TileCoord::new(clamp_tile(x), clamp_tile(y))
    };
    let start = warped(center);
    let cell = (
        start.x.div_euclid(OPENING_GRID_TILES),
        start.y.div_euclid(OPENING_GRID_TILES),
    );
    near_segment(start, node_center(generator, cell), warped(tile), 2)
}

fn near_segment(start: TileCoord, end: TileCoord, point: TileCoord, radius: i32) -> bool {
    let delta_x = i128::from(end.x) - i128::from(start.x);
    let delta_y = i128::from(end.y) - i128::from(start.y);
    let point_x = i128::from(point.x) - i128::from(start.x);
    let point_y = i128::from(point.y) - i128::from(start.y);
    let length_squared = delta_x * delta_x + delta_y * delta_y;
    let projection = point_x * delta_x + point_y * delta_y;
    let radius_squared = i128::from(radius) * i128::from(radius);
    if projection <= 0 {
        return point_x * point_x + point_y * point_y <= radius_squared;
    }
    if projection >= length_squared {
        let end_x = i128::from(point.x) - i128::from(end.x);
        let end_y = i128::from(point.y) - i128::from(end.y);
        return end_x * end_x + end_y * end_y <= radius_squared;
    }
    let cross = point_x * delta_y - point_y * delta_x;
    cross * cross <= radius_squared * length_squared
}

fn mix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests;
