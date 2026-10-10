//! Integer polygon geometry and keyed cell hashing for reservation shapes.
use super::MapChunkGenerator;
use aoe_core::TileCoord;

const VERTICES: usize = 16;
pub(super) const DIRECTION_SCALE: i64 = 4_096;
pub(super) const DIRECTIONS: [(i32, i32); VERTICES] = [
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

pub(super) fn cell_value(
    generator: &MapChunkGenerator,
    domain: &[u8],
    cell: (i32, i32),
    vertex: u8,
) -> u64 {
    let mut hash = blake3::Hasher::new_keyed(&generator.geography_key);
    hash.update(domain);
    hash.update(&cell.0.to_le_bytes());
    hash.update(&cell.1.to_le_bytes());
    hash.update(&vertex.to_le_bytes());
    u64::from_le_bytes(hash.finalize().as_bytes()[..8].try_into().unwrap_or([0; 8]))
}

pub(super) fn point_in_polygon(vertices: &[TileCoord], point: TileCoord) -> bool {
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
