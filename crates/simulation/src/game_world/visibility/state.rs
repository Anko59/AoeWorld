//! Per-player sight storage: exploration bitsets, dense visible counters and
//! the strategic overview. No allocation or iteration depends on map area.
use super::TileVisibility;
use aoe_core::{EntityId, TileCoord, TileRect};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Default)]
pub(super) struct PlayerVision {
    // 32x32 bits, allocated only after a tile in the chunk was observed.
    pub(super) explored: BTreeMap<(i32, i32), [u64; 16]>,
    // Only currently visible tiles carry reference counts, as dense 32x32
    // counters per chunk. Overlapping LOS cannot turn a tile fogged when just
    // one observer leaves.
    pub(super) visible: BTreeMap<(i32, i32), std::boxed::Box<[u32]>>,
    pub(super) visible_tiles: usize,
    pub(super) remembered: BTreeMap<aoe_core::EntityId, crate::GameBuilding>,
    /// Buildings whose entry in `remembered` was added, changed or dropped
    /// since a consumer last took the list. Never hashed.
    pub(super) changed: BTreeSet<EntityId>,
    /// Remembered buildings that no longer exist, by footprint tile. Seeing
    /// any of those tiles again ends the memory. Derived; never hashed.
    pub(super) ghost_tiles: HashMap<TileCoord, Vec<EntityId>>,
    pub(super) ghost_sites: BTreeMap<EntityId, TileRect>,
    pub(super) tile_work: u64,
    pub(super) overview: Overview,
    pub(super) resources: BTreeMap<u64, u16>,
    /// Bumped whenever this player's own line of sight changes.
    pub(super) revision: u64,
}

/// Per-pixel overview counts, dense per 256-pixel row and allocated on first
/// touch. Once touched, a pixel stays listed even when its count returns to
/// zero, so the reported set never depends on how counts moved since.
#[derive(Debug, Default)]
pub(super) struct Overview {
    rows: Vec<Option<OverviewRow>>,
}

#[derive(Debug)]
struct OverviewRow {
    counts: std::boxed::Box<[u32]>,
    touched: [u64; 4],
}

impl Overview {
    fn row(&mut self, row: usize) -> &mut OverviewRow {
        while self.rows.len() <= row {
            self.rows.push(None);
        }
        self.rows[row].get_or_insert_with(|| OverviewRow {
            counts: vec![0; 256].into_boxed_slice(),
            touched: [0; 4],
        })
    }

    pub(super) fn touch(&mut self, pixel: u16, add: bool) {
        let (row, column) = (usize::from(pixel) / 256, usize::from(pixel) % 256);
        let row = self.row(row);
        if add {
            row.counts[column] += 1;
        } else {
            row.counts[column] -= 1;
        }
        row.touched[column / 64] |= 1 << (column % 64);
    }

    pub(super) fn clear_counts(&mut self) {
        for row in self.rows.iter_mut().flatten() {
            row.counts = vec![0; 256].into_boxed_slice();
        }
    }

    /// Every touched pixel in ascending order with its current state.
    pub(super) fn states(&self) -> impl Iterator<Item = (u16, u8)> + '_ {
        self.rows.iter().enumerate().flat_map(|(index, slot)| {
            let Some(row) = slot else {
                return Box::new(std::iter::empty()) as Box<dyn Iterator<Item = (u16, u8)>>;
            };
            let columns = row
                .touched
                .iter()
                .enumerate()
                .flat_map(move |(word, bits)| {
                    (0..64)
                        .filter(move |bit| bits & (1 << bit) != 0)
                        .map(move |bit| word * 64 + bit)
                })
                .collect::<Vec<_>>();
            let states = columns.into_iter().map(move |column| {
                let pixel = (index * 256 + column) as u16;
                let state = if row.counts[column] > 0 { 2u8 } else { 1u8 };
                (pixel, state)
            });
            Box::new(states) as Box<dyn Iterator<Item = (u16, u8)>>
        })
    }
}

pub(super) fn address(tile: TileCoord) -> ((i32, i32), usize, u64) {
    let index = (tile.y.rem_euclid(32) * 32 + tile.x.rem_euclid(32)) as usize;
    (
        (tile.x.div_euclid(32), tile.y.div_euclid(32)),
        index / 64,
        1 << (index % 64),
    )
}

fn counter_index(tile: TileCoord) -> ((i32, i32), usize) {
    let index = (tile.y.rem_euclid(32) * 32 + tile.x.rem_euclid(32)) as usize;
    ((tile.x.div_euclid(32), tile.y.div_euclid(32)), index)
}

/// Every strategic pixel intersecting the tile, including tiles wider than a
/// pixel on supported small maps. Shared pixels retain reference counts.
pub(super) fn pixels(tile: i32, axis: i32) -> std::ops::Range<i64> {
    let (tile, axis) = (i64::from(tile), i64::from(axis));
    tile * 256 / axis..((tile + 1) * 256 + axis - 1) / axis
}

impl PlayerVision {
    /// Keeps a destroyed building's last sighting until its site is seen.
    pub(super) fn haunt(&mut self, id: EntityId, footprint: TileRect) {
        for y in footprint.min.y..footprint.max.y {
            for x in footprint.min.x..footprint.max.x {
                self.ghost_tiles
                    .entry(TileCoord::new(x, y))
                    .or_default()
                    .push(id);
            }
        }
        self.ghost_sites.insert(id, footprint);
    }

    /// Forgets a ghost whose site was just seen.
    pub(super) fn exorcise(&mut self, id: EntityId) {
        if self.remembered.remove(&id).is_some() {
            self.changed.insert(id);
        }
        let Some(footprint) = self.ghost_sites.remove(&id) else {
            return;
        };
        for y in footprint.min.y..footprint.max.y {
            for x in footprint.min.x..footprint.max.x {
                let tile = TileCoord::new(x, y);
                if let Some(ids) = self.ghost_tiles.get_mut(&tile) {
                    ids.retain(|ghost| *ghost != id);
                    if ids.is_empty() {
                        self.ghost_tiles.remove(&tile);
                    }
                }
            }
        }
    }

    pub(super) fn state(&self, tile: TileCoord) -> TileVisibility {
        let (chunk, index) = counter_index(tile);
        if self
            .visible
            .get(&chunk)
            .is_some_and(|counters| counters[index] > 0)
        {
            return TileVisibility::Visible;
        }
        let (chunk, word, mask) = address(tile);
        if self
            .explored
            .get(&chunk)
            .is_some_and(|bits| bits[word] & mask != 0)
        {
            TileVisibility::Explored
        } else {
            TileVisibility::Unexplored
        }
    }

    /// Raises one tile's reference count; true when the tile became visible.
    pub(super) fn raise_visible(&mut self, tile: TileCoord) -> bool {
        let (chunk, index) = counter_index(tile);
        let counters = self
            .visible
            .entry(chunk)
            .or_insert_with(|| vec![0; 1024].into_boxed_slice());
        counters[index] += 1;
        let fresh = counters[index] == 1;
        if fresh {
            self.visible_tiles += 1;
        }
        fresh
    }

    /// Drops one reference; true when the tile stopped being visible.
    pub(super) fn lower_visible(&mut self, tile: TileCoord) -> bool {
        let (chunk, index) = counter_index(tile);
        let Some(counters) = self.visible.get_mut(&chunk) else {
            return false;
        };
        if counters[index] == 0 {
            return false;
        }
        counters[index] -= 1;
        if counters[index] == 0 {
            self.visible_tiles -= 1;
            return true;
        }
        false
    }

    pub(super) fn set_explored(&mut self, tile: TileCoord) {
        let (chunk, word, mask) = address(tile);
        self.explored.entry(chunk).or_insert([0; 16])[word] |= mask;
    }

    pub(super) fn touch_overview(&mut self, tile: TileCoord, add: bool, width: i32, height: i32) {
        for y in pixels(tile.y, height) {
            for x in pixels(tile.x, width) {
                self.overview.touch((y * 256 + x) as u16, add);
            }
        }
    }
}
