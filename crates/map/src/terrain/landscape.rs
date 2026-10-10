use super::{GroundMaterial, MapChunkGenerator, clearing};
use crate::{biome_rules::Biome, biome_rules::forest_noise};
use aoe_core::TileCoord;

const CANOPY_GRID_TILES: i32 = 128;
const OPENING_GRID_TILES: i32 = 192;
const OPENING_CENTER_JITTER: i32 = 30;

impl MapChunkGenerator {
    /// Pure, bounded lookup of the procedural opening in this tile's cell.
    /// A node is not a claim that source water or cliffs are traversable.
    pub fn forest_opening_center_at(&self, tile: TileCoord) -> Option<TileCoord> {
        if !uses_reservation_geometry(self) || opening_geometry(self, tile).is_none() {
            return None;
        }
        let cell = (
            tile.x.div_euclid(OPENING_GRID_TILES),
            tile.y.div_euclid(OPENING_GRID_TILES),
        );
        Some(opening_center(cell_layout(self, cell), cell))
    }
}

pub(super) fn uses_forest_landscape(generator: &MapChunkGenerator) -> bool {
    matches!(
        generator.generation_recipe_version(),
        crate::PRIOR_FOREST_GENERATION_RECIPE_VERSION
            | crate::CONNECTED_FOREST_GENERATION_RECIPE_VERSION
    )
}

fn uses_reservation_geometry(generator: &MapChunkGenerator) -> bool {
    uses_forest_landscape(generator)
        || generator.generation_recipe_version() == crate::LANDSCAPE_GENERATION_RECIPE_VERSION
}

fn uses_connected_reservations(generator: &MapChunkGenerator) -> bool {
    matches!(
        generator.generation_recipe_version(),
        crate::CONNECTED_FOREST_GENERATION_RECIPE_VERSION
            | crate::LANDSCAPE_GENERATION_RECIPE_VERSION
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OpeningGeometry {
    vertices: [TileCoord; 16],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TrailSegment {
    start: TileCoord,
    bend: TileCoord,
    end: TileCoord,
    half_width: i32,
}

pub(crate) fn canopy_strength(key: [u8; 32], x: i32, y: i32) -> u16 {
    interpolated_field(key, b"forest-canopy-lattice-v1", x, y, CANOPY_GRID_TILES)
}

fn interpolated_field(key: [u8; 32], domain: &[u8], x: i32, y: i32, grid: i32) -> u16 {
    let cell_x = x.div_euclid(grid);
    let cell_y = y.div_euclid(grid);
    let offset_x = x.rem_euclid(grid);
    let offset_y = y.rem_euclid(grid);
    let northwest = field_node(key, domain, cell_x, cell_y);
    let northeast = field_node(key, domain, cell_x.saturating_add(1), cell_y);
    let southwest = field_node(key, domain, cell_x, cell_y.saturating_add(1));
    let southeast = field_node(
        key,
        domain,
        cell_x.saturating_add(1),
        cell_y.saturating_add(1),
    );
    let north = interpolate(northwest, northeast, offset_x, grid);
    let south = interpolate(southwest, southeast, offset_x, grid);
    interpolate(north, south, offset_y, grid)
}

fn field_node(key: [u8; 32], domain: &[u8], x: i32, y: i32) -> u16 {
    (forest_noise(key, domain, x, y) % 1_001) as u16
}

fn interpolate(left: u16, right: u16, offset: i32, grid: i32) -> u16 {
    let scale = i64::from(grid);
    let offset = i64::from(offset);
    ((i64::from(left) * (scale - offset) + i64::from(right) * offset) / scale) as u16
}

pub(super) fn opening_contains(generator: &MapChunkGenerator, tile: TileCoord) -> bool {
    starting_glade(generator, tile)
        || opening_geometry(generator, tile)
            .is_some_and(|shape| clearing::point_in_polygon(&shape.vertices, tile))
}

fn starting_glade(generator: &MapChunkGenerator, tile: TileCoord) -> bool {
    if !uses_reservation_geometry(generator) {
        return false;
    }
    let width = generator.width_tiles.max(1);
    let center = TileCoord::new((width - 1) / 2, (width - 1) / 2);
    if tile.x.abs_diff(center.x) > 27 || tile.y.abs_diff(center.y) > 27 {
        return false;
    }
    let layout = clearing::cell_value(generator, b"forest-starting-glade-v1", (width, 0), 0);
    let base_radius = 20 + ((layout >> 12) % 5) as i32;
    let x_scale = 88 + ((mix64(layout ^ 0x2a5d_3c91) >> 11) % 13) as i32;
    let y_scale = 88 + ((mix64(layout ^ 0x7c4a_1e63) >> 9) % 13) as i32;
    let vertices: [TileCoord; 16] = std::array::from_fn(|vertex| {
        let (direction_x, direction_y) = clearing::DIRECTIONS[vertex];
        let jitter = ((mix64(layout ^ vertex as u64) >> 17) % 5) as i32 - 2;
        let radius = base_radius + jitter;
        let offset_x = i64::from(direction_x) * i64::from(radius) * i64::from(x_scale)
            / (clearing::DIRECTION_SCALE * 100);
        let offset_y = i64::from(direction_y) * i64::from(radius) * i64::from(y_scale)
            / (clearing::DIRECTION_SCALE * 100);
        TileCoord::new(
            center
                .x
                .saturating_add(i32::try_from(offset_x).unwrap_or(0)),
            center
                .y
                .saturating_add(i32::try_from(offset_y).unwrap_or(0)),
        )
    });
    clearing::point_in_polygon(&vertices, tile)
}

fn opening_geometry(generator: &MapChunkGenerator, tile: TileCoord) -> Option<OpeningGeometry> {
    opening_geometry_for_cell(
        generator,
        (
            tile.x.div_euclid(OPENING_GRID_TILES),
            tile.y.div_euclid(OPENING_GRID_TILES),
        ),
    )
}

fn opening_geometry_for_cell(
    generator: &MapChunkGenerator,
    cell: (i32, i32),
) -> Option<OpeningGeometry> {
    let layout = cell_layout(generator, cell);
    let kind = layout % 1_000;
    let minimum_radius = if kind < 300 {
        19
    } else if kind < 760 {
        31
    } else if uses_connected_reservations(generator) {
        // Small mandatory nodes join the recipe-eight graph without reseeding
        // the already established larger openings or canopy.
        19
    } else {
        return None;
    };
    let radius_span = if minimum_radius == 19 { 13 } else { 22 };
    let base_radius = minimum_radius + ((layout >> 12) % radius_span) as i32;
    let center = opening_center(layout, cell);
    let x_scale = 70 + ((mix64(layout ^ 0x2a5d_3c91) >> 11) % 43) as i32;
    let y_scale = 70 + ((mix64(layout ^ 0x7c4a_1e63) >> 9) % 43) as i32;
    let radii: [i32; 16] = std::array::from_fn(|vertex| {
        let jitter = ((mix64(layout ^ vertex as u64) >> 17) % 9) as i32 - 4;
        base_radius + jitter
    });
    let vertices = std::array::from_fn(|vertex| {
        let (direction_x, direction_y) = clearing::DIRECTIONS[vertex];
        let offset_x = i64::from(direction_x) * i64::from(radii[vertex]) * i64::from(x_scale)
            / (clearing::DIRECTION_SCALE * 100);
        let offset_y = i64::from(direction_y) * i64::from(radii[vertex]) * i64::from(y_scale)
            / (clearing::DIRECTION_SCALE * 100);
        TileCoord::new(
            center
                .x
                .saturating_add(i32::try_from(offset_x).unwrap_or(0)),
            center
                .y
                .saturating_add(i32::try_from(offset_y).unwrap_or(0)),
        )
    });
    Some(OpeningGeometry { vertices })
}

fn cell_layout(generator: &MapChunkGenerator, cell: (i32, i32)) -> u64 {
    clearing::cell_value(generator, b"forest-opening-layout-v1", cell, 0)
}

fn opening_center(layout: u64, cell: (i32, i32)) -> TileCoord {
    let offset = |value: u64| {
        (value % u64::from((OPENING_CENTER_JITTER * 2 + 1) as u8)) as i32 - OPENING_CENTER_JITTER
    };
    TileCoord::new(
        cell.0
            .saturating_mul(OPENING_GRID_TILES)
            .saturating_add(OPENING_GRID_TILES / 2)
            .saturating_add(offset(mix64(layout ^ 0x4d83_1b27))),
        cell.1
            .saturating_mul(OPENING_GRID_TILES)
            .saturating_add(OPENING_GRID_TILES / 2)
            .saturating_add(offset(mix64(layout ^ 0x9e31_705b))),
    )
}

pub(super) fn procedural_trail_contains(generator: &MapChunkGenerator, tile: TileCoord) -> bool {
    if !uses_reservation_geometry(generator) {
        return false;
    }
    if starting_connector_contains(generator, tile) {
        return true;
    }
    let cell_x = tile.x.div_euclid(OPENING_GRID_TILES);
    let cell_y = tile.y.div_euclid(OPENING_GRID_TILES);
    [
        ((cell_x, cell_y), 0),
        ((cell_x.saturating_sub(1), cell_y), 0),
        ((cell_x, cell_y), 1),
        ((cell_x, cell_y.saturating_sub(1)), 1),
    ]
    .into_iter()
    .filter_map(|(cell, axis)| trail_segment(generator, cell, axis))
    .any(|segment| {
        near_segment(segment.start, segment.bend, tile, segment.half_width)
            || near_segment(segment.bend, segment.end, tile, segment.half_width)
    })
}

fn trail_segment(
    generator: &MapChunkGenerator,
    cell: (i32, i32),
    axis: u8,
) -> Option<TrailSegment> {
    let start_layout = cell_layout(generator, cell);
    let connection = mix64(start_layout ^ (u64::from(axis) << 48));
    let legacy =
        generator.generation_recipe_version() == crate::PRIOR_FOREST_GENERATION_RECIPE_VERSION;
    if legacy && (connection % 100 >= 88 || start_layout % 1_000 >= 760) {
        return None;
    }
    let neighbor = if axis == 0 {
        (cell.0.saturating_add(1), cell.1)
    } else {
        (cell.0, cell.1.saturating_add(1))
    };
    let end_layout = cell_layout(generator, neighbor);
    if legacy && end_layout % 1_000 >= 760 {
        return None;
    }
    let start = opening_center(start_layout, cell);
    let end = opening_center(end_layout, neighbor);
    let sign = if connection & 1 == 0 { -1 } else { 1 };
    let bend_offset = sign * (7 + ((connection >> 19) % 13) as i32);
    let bend = if axis == 0 {
        TileCoord::new(
            midpoint(start.x, end.x),
            midpoint(start.y, end.y).saturating_add(bend_offset),
        )
    } else {
        TileCoord::new(
            midpoint(start.x, end.x).saturating_add(bend_offset),
            midpoint(start.y, end.y),
        )
    };
    Some(TrailSegment {
        start,
        bend,
        end,
        half_width: 2 + ((connection >> 32) % 2) as i32,
    })
}

fn starting_connector_contains(generator: &MapChunkGenerator, tile: TileCoord) -> bool {
    if !uses_connected_reservations(generator) {
        return false;
    }
    let coordinate = (generator.width_tiles.max(1) - 1) / 2;
    let center = TileCoord::new(coordinate, coordinate);
    // The center and its node are in one 192-tile cell. Reject far-away tiles
    // before hashing; no map-wide allocation or source search during sampling.
    let reach = if generator.uses_landscape_v2() {
        OPENING_GRID_TILES + OPENING_GRID_TILES / 2 + OPENING_CENTER_JITTER
    } else {
        OPENING_GRID_TILES
    };
    if tile.x.abs_diff(center.x) > reach as u32 || tile.y.abs_diff(center.y) > reach as u32 {
        return false;
    }
    let cell = (
        coordinate.div_euclid(OPENING_GRID_TILES),
        coordinate.div_euclid(OPENING_GRID_TILES),
    );
    if generator.uses_landscape_v2() {
        // Dense coherent forests cannot rely on recipe-eight's permeable
        // scattered trees as shortcuts through bent graph edges. Connect the
        // starting glade directly to its seeded surrounding opening nodes so a
        // bounded planner can leave toward a destination without that detour.
        // This remains only a shared vegetation reservation: cliffs/water stay.
        for dy in -1..=1 {
            for dx in -1..=1 {
                let neighbor = (cell.0.saturating_add(dx), cell.1.saturating_add(dy));
                let end = opening_center(cell_layout(generator, neighbor), neighbor);
                if near_segment(center, end, tile, 3) {
                    return true;
                }
            }
        }
        return false;
    }
    let end = opening_center(cell_layout(generator, cell), cell);
    near_segment(center, end, tile, 3)
}

fn midpoint(left: i32, right: i32) -> i32 {
    (i64::from(left) + i64::from(right)).div_euclid(2) as i32
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

pub(super) fn material_for_tile(
    generator: &MapChunkGenerator,
    tile: TileCoord,
    biome: Biome,
    base: GroundMaterial,
) -> GroundMaterial {
    if !uses_forest_landscape(generator)
        || biome != Biome::Temperate
        || base != GroundMaterial::TemperateGrass
    {
        return base;
    }
    if procedural_trail_contains(generator, tile) {
        return GroundMaterial::Dirt;
    }
    if opening_contains(generator, tile) {
        let patch = interpolated_field(
            generator.geography_key,
            b"forest-opening-ground-v1",
            tile.x,
            tile.y,
            28,
        );
        return if patch >= 900 {
            GroundMaterial::Dirt
        } else {
            GroundMaterial::DryGrass
        };
    }
    let canopy = canopy_strength(generator.geography_key, tile.x, tile.y);
    if canopy >= 620 {
        return GroundMaterial::ForestFloor;
    }
    let patch = interpolated_field(
        generator.geography_key,
        b"forest-margin-ground-v1",
        tile.x,
        tile.y,
        28,
    );
    if (380..620).contains(&canopy) && patch >= 900 {
        GroundMaterial::DryGrass
    } else {
        base
    }
}

fn mix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests;
