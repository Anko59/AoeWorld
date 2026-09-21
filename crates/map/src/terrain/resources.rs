use super::{MapChunkGenerator, ObjectKind, ResourceKind, ResourceNode, Tile, resource_id};
use crate::Biome;
use aoe_core::TileCoord;

const FORAGE_COLUMNS: i32 = 64;
const FORAGE_ROWS: i32 = 64;
const GOLD_COLUMNS: i32 = 192;
const GOLD_ROWS: i32 = 96;
const STONE_COLUMNS: i32 = 96;
const STONE_ROWS: i32 = 48;
const PATCH_OFFSETS: [(i32, i32); 8] = [
    (0, 0),
    (1, 0),
    (0, 1),
    (-1, 0),
    (0, -1),
    (1, 1),
    (-1, -1),
    (1, -1),
];

pub(super) fn at(
    generator: &MapChunkGenerator,
    tile: TileCoord,
    sample: Tile,
) -> Option<ResourceNode> {
    let candidate = candidate(generator, tile, sample)?;
    adjacent_access(generator, tile).then_some(candidate)
}

pub(super) fn candidate(
    generator: &MapChunkGenerator,
    tile: TileCoord,
    sample: Tile,
) -> Option<ResourceNode> {
    if !sample.passable {
        return None;
    }
    let key = detail_key(generator);
    if suitable_forage(sample.biome)
        && let Some((slot, key)) =
            patch_node(key, b"forage-patch", tile, FORAGE_COLUMNS, FORAGE_ROWS)
    {
        return Some(node(
            tile,
            ResourceKind::Food,
            ObjectKind::ForageBush,
            125,
            slot,
            key,
        ));
    }
    if !suitable_ore(sample.biome) {
        return None;
    }
    if let Some((slot, key)) = patch_node(key, b"gold-patch-v2", tile, GOLD_COLUMNS, GOLD_ROWS) {
        return Some(node(
            tile,
            ResourceKind::Gold,
            ObjectKind::GoldDeposit,
            800,
            slot,
            key,
        ));
    }
    let (slot, key) = patch_node(key, b"stone-patch-v2", tile, STONE_COLUMNS, STONE_ROWS)?;
    Some(node(
        tile,
        ResourceKind::Stone,
        ObjectKind::StoneDeposit,
        350,
        slot,
        key,
    ))
}

pub(super) fn detail_key(generator: &MapChunkGenerator) -> [u8; 32] {
    let mut hash = blake3::Hasher::new_keyed(&generator.geography_key);
    hash.update(b"resource-detail-v2");
    hash.update(&crate::GENERATION_RECIPE_VERSION.to_le_bytes());
    hash.update(&generator.procedural_seed.to_le_bytes());
    *hash.finalize().as_bytes()
}

fn node(
    tile: TileCoord,
    kind: ResourceKind,
    object: ObjectKind,
    initial_amount: u16,
    slot: u8,
    key: u64,
) -> ResourceNode {
    ResourceNode {
        id: resource_id(tile, 0),
        tile,
        kind,
        object,
        initial_amount,
        visual_variant: (key.rotate_right(u32::from(slot)) >> 8) as u8,
    }
}

fn patch_node(
    key: [u8; 32],
    domain: &[u8],
    tile: TileCoord,
    columns: i32,
    rows: i32,
) -> Option<(u8, u64)> {
    let cell_x = tile.x.div_euclid(columns);
    let cell_y = tile.y.div_euclid(rows);
    for y in cell_y.saturating_sub(1)..=cell_y.saturating_add(1) {
        for x in cell_x.saturating_sub(1)..=cell_x.saturating_add(1) {
            let (root, value, count) = patch_origin(key, domain, x, y, columns, rows);
            for (slot, &(offset_x, offset_y)) in PATCH_OFFSETS[..count].iter().enumerate() {
                if tile
                    == TileCoord::new(
                        root.x.saturating_add(offset_x),
                        root.y.saturating_add(offset_y),
                    )
                {
                    return Some((slot as u8, value));
                }
            }
        }
    }
    None
}

pub(super) fn patch_origin(
    key: [u8; 32],
    domain: &[u8],
    x: i32,
    y: i32,
    columns: i32,
    rows: i32,
) -> (TileCoord, u64, usize) {
    let value = super::unsigned_noise(key, domain, x, y);
    let root = TileCoord::new(
        x.saturating_mul(columns) + (value % columns as u64) as i32,
        y.saturating_mul(rows) + (value.rotate_left(13) % rows as u64) as i32,
    );
    (root, value, patch_count(value.rotate_left(29)))
}

fn patch_count(value: u64) -> usize {
    4 + (value % 5) as usize
}

fn adjacent_access(generator: &MapChunkGenerator, tile: TileCoord) -> bool {
    [
        TileCoord::new(tile.x - 1, tile.y),
        TileCoord::new(tile.x + 1, tile.y),
        TileCoord::new(tile.x, tile.y - 1),
        TileCoord::new(tile.x, tile.y + 1),
    ]
    .into_iter()
    .any(|neighbor| {
        generator.tile_at(neighbor).is_some_and(|sample| {
            sample.passable && !generator.occupied_without_access(neighbor, sample)
        })
    })
}

fn suitable_forage(biome: Biome) -> bool {
    matches!(
        biome,
        Biome::Tropical | Biome::Temperate | Biome::Boreal | Biome::Woodland | Biome::Savanna
    )
}

fn suitable_ore(biome: Biome) -> bool {
    !matches!(biome, Biome::Tundra | Biome::Alpine | Biome::Polar)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patches_are_bounded_and_identical_from_every_query_direction() {
        let key = [7; 32];
        let nodes = (-256..256)
            .flat_map(|y| {
                (-256..256).filter_map(move |x| {
                    patch_node(
                        key,
                        b"stone-patch-v2",
                        TileCoord::new(x, y),
                        STONE_COLUMNS,
                        STONE_ROWS,
                    )
                    .map(|(slot, _)| (x, y, slot))
                })
            })
            .collect::<Vec<_>>();
        assert!(nodes.windows(2).all(|pair| pair[0] != pair[1]));
        assert!(nodes.iter().all(|(_, _, slot)| *slot < 8));
    }

    #[test]
    fn forage_patch_candidates_are_always_between_four_and_eight() {
        let counts = (0u64..256)
            .map(|value| patch_count(value.rotate_left(29)))
            .collect::<Vec<_>>();
        assert_eq!(counts.iter().copied().min(), Some(4));
        assert_eq!(counts.iter().copied().max(), Some(8));
    }

    #[test]
    fn gold_and_stone_use_independent_streams_with_distinct_spacing() {
        let key = [11; 32];
        let gold_domain = b"gold-patch-v2";
        let stone_domain = b"stone-patch-v2";
        assert_ne!(
            super::super::unsigned_noise(key, gold_domain, 3, 5),
            super::super::unsigned_noise(key, stone_domain, 3, 5)
        );
        let (gold_root, _, _) = patch_origin(key, gold_domain, 3, 5, GOLD_COLUMNS, GOLD_ROWS);
        let (stone_root, _, _) = patch_origin(key, stone_domain, 3, 5, STONE_COLUMNS, STONE_ROWS);
        assert_ne!(gold_root, stone_root);
    }
}
