use super::{MapChunkGenerator, ObjectKind, ResourceKind, ResourceNode, Tile, resource_id};
use crate::Biome;
use aoe_core::TileCoord;

const FORAGE_COLUMNS: i32 = 64;
const FORAGE_ROWS: i32 = 64;
const ORE_COLUMNS: i32 = 128;
const ORE_ROWS: i32 = 64;
const PATCH_OFFSETS: [(i32, i32); 7] = [(0, 0), (1, 0), (0, 1), (-1, 0), (0, -1), (1, 1), (-1, -1)];

pub(super) fn at(
    generator: &MapChunkGenerator,
    tile: TileCoord,
    sample: Tile,
) -> Option<ResourceNode> {
    let key = detail_key(generator);
    if suitable_forage(sample.biome)
        && let Some((slot, key)) =
            patch_node(key, b"forage-patch", tile, FORAGE_COLUMNS, FORAGE_ROWS)
    {
        return node(
            generator,
            tile,
            ResourceKind::Food,
            ObjectKind::ForageBush,
            125,
            slot,
            key,
        );
    }
    if !suitable_ore(sample.biome) {
        return None;
    }
    let (slot, key) = patch_node(key, b"ore-patch", tile, ORE_COLUMNS, ORE_ROWS)?;
    let gold = key.is_multiple_of(2);
    node(
        generator,
        tile,
        if gold {
            ResourceKind::Gold
        } else {
            ResourceKind::Stone
        },
        if gold {
            ObjectKind::GoldDeposit
        } else {
            ObjectKind::StoneDeposit
        },
        if gold { 800 } else { 350 },
        slot,
        key,
    )
}

fn detail_key(generator: &MapChunkGenerator) -> [u8; 32] {
    let mut hash = blake3::Hasher::new_keyed(&generator.geography_key);
    hash.update(b"resource-detail-v1");
    hash.update(&generator.procedural_seed.to_le_bytes());
    *hash.finalize().as_bytes()
}

fn node(
    generator: &MapChunkGenerator,
    tile: TileCoord,
    kind: ResourceKind,
    object: ObjectKind,
    initial_amount: u16,
    slot: u8,
    key: u64,
) -> Option<ResourceNode> {
    adjacent_access(generator, tile).then_some(ResourceNode {
        id: resource_id(tile, 0),
        tile,
        kind,
        object,
        initial_amount,
        visual_variant: (key.rotate_right(u32::from(slot)) >> 8) as u8,
    })
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
            let value = super::unsigned_noise(key, domain, x, y);
            let root = TileCoord::new(
                x.saturating_mul(columns) + (value % columns as u64) as i32,
                y.saturating_mul(rows) + (value.rotate_left(13) % rows as u64) as i32,
            );
            let count = 4 + (value.rotate_left(29) % 4) as usize;
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

fn adjacent_access(generator: &MapChunkGenerator, tile: TileCoord) -> bool {
    [
        TileCoord::new(tile.x - 1, tile.y),
        TileCoord::new(tile.x + 1, tile.y),
        TileCoord::new(tile.x, tile.y - 1),
        TileCoord::new(tile.x, tile.y + 1),
    ]
    .into_iter()
    .any(|neighbor| {
        generator
            .tile_at(neighbor)
            .is_some_and(|sample| sample.passable)
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
                        b"ore-patch",
                        TileCoord::new(x, y),
                        ORE_COLUMNS,
                        ORE_ROWS,
                    )
                    .map(|(slot, _)| (x, y, slot))
                })
            })
            .collect::<Vec<_>>();
        assert!(nodes.windows(2).all(|pair| pair[0] != pair[1]));
        assert!(nodes.iter().all(|(_, _, slot)| *slot < 7));
    }
}
