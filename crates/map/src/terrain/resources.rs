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

/// Base resource distribution for the shared landscape mask. Does not query
/// decorated tiles or trees; callers own reservations and access.
#[cfg(test)]
pub(super) fn candidate_unreserved(
    generator: &MapChunkGenerator,
    tile: TileCoord,
    sample: Tile,
) -> Option<ResourceNode> {
    Origins::new(generator).candidate(tile, sample)
}

/// Patch domain and cell coordinates.
type CellKey = (u8, i32, i32);
/// Patch root, keyed value and member count.
type Origin = (TileCoord, u64, usize);

/// Query-local memo of patch-cell origins. Neighboring candidate queries in
/// one point evaluation share cells; results equal uncached evaluation.
pub(super) struct Origins {
    key: [u8; 32],
    cells: Vec<(CellKey, Origin)>,
}

impl Origins {
    pub(super) fn new(generator: &MapChunkGenerator) -> Self {
        Self {
            key: detail_key(generator),
            cells: Vec::new(),
        }
    }

    #[cfg(test)]
    pub(super) fn candidate(&mut self, tile: TileCoord, sample: Tile) -> Option<ResourceNode> {
        self.candidate_with(tile, || Ok::<_, std::convert::Infallible>(Some(sample)))
            .unwrap_or_else(|never| match never {})
    }

    /// Patch membership is pure coordinate hashing, so the base tile is read
    /// only for the few tiles inside a patch; results equal eager evaluation.
    pub(super) fn candidate_with<E>(
        &mut self,
        tile: TileCoord,
        base: impl FnOnce() -> Result<Option<Tile>, E>,
    ) -> Result<Option<ResourceNode>, E> {
        let forage = self.patch_node(0, tile, FORAGE_COLUMNS, FORAGE_ROWS);
        let gold = self.patch_node(1, tile, GOLD_COLUMNS, GOLD_ROWS);
        let stone = self.patch_node(2, tile, STONE_COLUMNS, STONE_ROWS);
        if forage.is_none() && gold.is_none() && stone.is_none() {
            return Ok(None);
        }
        let Some(sample) = base()? else {
            return Ok(None);
        };
        if !sample.passable {
            return Ok(None);
        }
        if suitable_forage(sample.biome)
            && let Some((slot, key)) = forage
        {
            return Ok(Some(node(
                tile,
                ResourceKind::Food,
                ObjectKind::ForageBush,
                125,
                slot,
                key,
            )));
        }
        if !suitable_ore(sample.biome) {
            return Ok(None);
        }
        if let Some((slot, key)) = gold {
            return Ok(Some(node(
                tile,
                ResourceKind::Gold,
                ObjectKind::GoldDeposit,
                800,
                slot,
                key,
            )));
        }
        Ok(stone.map(|(slot, key)| {
            node(
                tile,
                ResourceKind::Stone,
                ObjectKind::StoneDeposit,
                350,
                slot,
                key,
            )
        }))
    }

    fn origin(&mut self, domain: u8, x: i32, y: i32, columns: i32, rows: i32) -> Origin {
        if let Some((_, origin)) = self.cells.iter().find(|(cell, _)| *cell == (domain, x, y)) {
            return *origin;
        }
        let origin = patch_origin(self.key, DOMAINS[usize::from(domain)], x, y, columns, rows);
        self.cells.push(((domain, x, y), origin));
        origin
    }

    fn patch_node(
        &mut self,
        domain: u8,
        tile: TileCoord,
        columns: i32,
        rows: i32,
    ) -> Option<(u8, u64)> {
        let (xs, ys) = candidate_cells(tile, columns, rows);
        for y in ys {
            for x in xs.clone() {
                let (root, value, count) = self.origin(domain, x, y, columns, rows);
                if let Some(slot) = patch_slot(tile, root, count) {
                    return Some((slot, value));
                }
            }
        }
        None
    }
}

/// Patch roots lie inside their cell and members are at most one tile from
/// the root, so only edge tiles can belong to a neighboring cell's patch.
/// Cells keep the canonical row-major order of the full 3×3 scan.
fn candidate_cells(
    tile: TileCoord,
    columns: i32,
    rows: i32,
) -> (std::ops::RangeInclusive<i32>, std::ops::RangeInclusive<i32>) {
    let axis = |coordinate: i32, size: i32| {
        let cell = coordinate.div_euclid(size);
        let local = coordinate.rem_euclid(size);
        let first = if local == 0 {
            cell.saturating_sub(1)
        } else {
            cell
        };
        let last = if local == size - 1 {
            cell.saturating_add(1)
        } else {
            cell
        };
        first..=last
    };
    (axis(tile.x, columns), axis(tile.y, rows))
}

const DOMAINS: [&[u8]; 3] = [b"forage-patch", b"gold-patch-v2", b"stone-patch-v2"];

fn patch_slot(tile: TileCoord, root: TileCoord, count: usize) -> Option<u8> {
    PATCH_OFFSETS[..count]
        .iter()
        .position(|&(offset_x, offset_y)| {
            tile == TileCoord::new(
                root.x.saturating_add(offset_x),
                root.y.saturating_add(offset_y),
            )
        })
        .map(|slot| slot as u8)
}

pub(super) fn detail_key(generator: &MapChunkGenerator) -> [u8; 32] {
    let mut hash = blake3::Hasher::new_keyed(&generator.geography_key);
    hash.update(b"resource-detail");
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

#[cfg(test)]
fn patch_node(
    key: [u8; 32],
    domain: &[u8],
    tile: TileCoord,
    columns: i32,
    rows: i32,
) -> Option<(u8, u64)> {
    let (xs, ys) = candidate_cells(tile, columns, rows);
    for y in ys {
        for x in xs.clone() {
            let (root, value, count) = patch_origin(key, domain, x, y, columns, rows);
            if let Some(slot) = patch_slot(tile, root, count) {
                return Some((slot, value));
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
) -> Origin {
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
    fn edge_restricted_cell_scan_equals_the_full_neighborhood_scan() {
        let full = |key, domain: &[u8], tile: TileCoord, columns: i32, rows: i32| {
            let (cx, cy) = (tile.x.div_euclid(columns), tile.y.div_euclid(rows));
            for y in cy - 1..=cy + 1 {
                for x in cx - 1..=cx + 1 {
                    let (root, value, count) = patch_origin(key, domain, x, y, columns, rows);
                    if let Some(slot) = patch_slot(tile, root, count) {
                        return Some((slot, value));
                    }
                }
            }
            None
        };
        for key in [[7; 32], [19; 32]] {
            for (domain, columns, rows) in [
                (DOMAINS[0], FORAGE_COLUMNS, FORAGE_ROWS),
                (DOMAINS[1], GOLD_COLUMNS, GOLD_ROWS),
                (DOMAINS[2], STONE_COLUMNS, STONE_ROWS),
            ] {
                let mut hits = 0;
                for y in -200..200 {
                    for x in -200..200 {
                        let tile = TileCoord::new(x, y);
                        let expected = full(key, domain, tile, columns, rows);
                        hits += usize::from(expected.is_some());
                        assert_eq!(patch_node(key, domain, tile, columns, rows), expected);
                    }
                }
                assert!(hits > 0);
            }
        }
    }

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
