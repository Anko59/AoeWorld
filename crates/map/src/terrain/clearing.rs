use super::MapChunkGenerator;
use aoe_core::TileCoord;

pub(super) const CELL_TILES: i32 = 96;
#[cfg(test)]
pub(super) const CORE_TILES: i32 = 17;

const CENTER_JITTER: i32 = 12;
const MIN_RADIUS: i32 = 24;
const MAX_RADIUS: i32 = 31;
const VERTICES: usize = 16;
const DIRECTION_SCALE: i64 = 4_096;
const DIRECTIONS: [(i32, i32); VERTICES] = [
    (4_096, 0),
    (3_783, 1_583),
    (2_896, 2_896),
    (1_583, 3_783),
    (0, 4_096),
    (-1_583, 3_783),
    (-2_896, 2_896),
    (-3_783, 1_583),
    (-4_096, 0),
    (-3_783, -1_583),
    (-2_896, -2_896),
    (-1_583, -3_783),
    (0, -4_096),
    (1_583, -3_783),
    (2_896, -2_896),
    (3_783, -1_583),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Geometry {
    pub(super) cell: (i32, i32),
    pub(super) center: TileCoord,
    pub(super) radii: [i32; VERTICES],
    pub(super) vertices: [TileCoord; VERTICES],
}

pub(super) fn contains(generator: &MapChunkGenerator, tile: TileCoord) -> bool {
    geometry(generator, tile).is_some_and(|shape| point_in_polygon(&shape.vertices, tile))
}

pub(super) fn geometry(generator: &MapChunkGenerator, tile: TileCoord) -> Option<Geometry> {
    if generator.generation_recipe_version() != crate::GENERATION_RECIPE_VERSION {
        return None;
    }
    let cell = (tile.x.div_euclid(CELL_TILES), tile.y.div_euclid(CELL_TILES));
    let center = center(generator, cell);
    let mut radii = [0_i32; VERTICES];
    for (vertex, radius) in radii.iter_mut().enumerate() {
        let value = cell_value(
            generator,
            b"forest-clearing-shape",
            cell,
            u8::try_from(vertex).unwrap_or(u8::MAX),
        );
        *radius = MIN_RADIUS
            + i32::try_from(value % u64::from((MAX_RADIUS - MIN_RADIUS + 1) as u8)).unwrap_or(0);
    }
    if radii.iter().all(|radius| *radius == radii[0]) {
        radii[0] = MIN_RADIUS;
        radii[1] = MAX_RADIUS;
    }
    let vertices = std::array::from_fn(|vertex| {
        let (direction_x, direction_y) = DIRECTIONS[vertex];
        let offset_x = i64::from(direction_x) * i64::from(radii[vertex]) / DIRECTION_SCALE;
        let offset_y = i64::from(direction_y) * i64::from(radii[vertex]) / DIRECTION_SCALE;
        TileCoord::new(
            center
                .x
                .saturating_add(i32::try_from(offset_x).unwrap_or(0)),
            center
                .y
                .saturating_add(i32::try_from(offset_y).unwrap_or(0)),
        )
    });
    Some(Geometry {
        cell,
        center,
        radii,
        vertices,
    })
}

fn center(generator: &MapChunkGenerator, cell: (i32, i32)) -> TileCoord {
    let value = cell_value(generator, b"forest-clearing-center", cell, 0);
    let offset = |part: u64| {
        i32::try_from(part % u64::from((CENTER_JITTER * 2 + 1) as u8)).unwrap_or(0) - CENTER_JITTER
    };
    TileCoord::new(
        cell.0
            .saturating_mul(CELL_TILES)
            .saturating_add(CELL_TILES / 2)
            .saturating_add(offset(value)),
        cell.1
            .saturating_mul(CELL_TILES)
            .saturating_add(CELL_TILES / 2)
            .saturating_add(offset(value.rotate_left(32))),
    )
}

fn cell_value(generator: &MapChunkGenerator, domain: &[u8], cell: (i32, i32), vertex: u8) -> u64 {
    let mut hash = blake3::Hasher::new_keyed(&generator.geography_key);
    hash.update(domain);
    hash.update(&cell.0.to_le_bytes());
    hash.update(&cell.1.to_le_bytes());
    hash.update(&vertex.to_le_bytes());
    u64::from_le_bytes(hash.finalize().as_bytes()[..8].try_into().unwrap_or([0; 8]))
}

fn point_in_polygon(vertices: &[TileCoord], point: TileCoord) -> bool {
    let mut inside = false;
    for (index, left) in vertices.iter().enumerate() {
        let right = vertices[(index + 1) % vertices.len()];
        if point_on_segment(*left, right, point) {
            return true;
        }
        if (left.y > point.y) != (right.y > point.y) {
            let numerator = i64::from(point.y - left.y) * i64::from(right.x - left.x)
                - i64::from(point.x - left.x) * i64::from(right.y - left.y);
            let denominator = i64::from(right.y - left.y);
            if (denominator > 0 && numerator > 0) || (denominator < 0 && numerator < 0) {
                inside = !inside;
            }
        }
    }
    inside
}

fn point_on_segment(left: TileCoord, right: TileCoord, point: TileCoord) -> bool {
    let cross = i64::from(point.x - left.x) * i64::from(right.y - left.y)
        - i64::from(point.y - left.y) * i64::from(right.x - left.x);
    cross == 0
        && point.x >= left.x.min(right.x)
        && point.x <= left.x.max(right.x)
        && point.y >= left.y.min(right.y)
        && point.y <= left.y.max(right.y)
}
