//! What the slot remembers of the map's fog. The server sends the state of
//! the subscribed region; the SDK keeps older regions so a bot that moves its
//! view still knows what it explored.
use crate::model::Visibility;
use aoe_core::{TileCoord, TileRect};
use aoe_protocol::FogState;
use std::collections::BTreeMap;

const CHUNK: i32 = 32;

/// The server's strategic overview is 256 x 256 pixels over the whole map.
const OVERVIEW_AXIS: i64 = 256;

#[derive(Clone, Debug)]
pub struct FogMemory {
    region: Option<TileRect>,
    live: Vec<u8>,
    /// Explored chunks: 0 unexplored, 1 explored (never downgraded).
    explored: BTreeMap<(i32, i32), Box<[u8; 1024]>>,
    disabled: bool,
    overview: Vec<(u16, u8)>,
    /// Pixels of the strategic overview ever seen explored: the slot's whole
    /// map memory at coarse resolution, whatever window it subscribed to.
    overview_explored: Box<[bool; 65_536]>,
    size: (i32, i32),
}

impl Default for FogMemory {
    fn default() -> Self {
        Self {
            region: None,
            live: Vec::new(),
            explored: BTreeMap::new(),
            disabled: false,
            overview: Vec::new(),
            overview_explored: Box::new([false; 65_536]),
            size: (0, 0),
        }
    }
}

impl FogMemory {
    pub(crate) fn apply(&mut self, fog: &FogState, size: (i32, i32)) {
        self.disabled = fog.disabled;
        self.size = size;
        self.overview = fog.overview.clone();
        for (pixel, _) in &fog.overview {
            self.overview_explored[usize::from(*pixel)] = true;
        }
        if fog.disabled {
            self.region = Some(fog.region);
            self.live.clear();
            return;
        }
        let width = fog.region.width();
        for (index, state) in fog.tiles.iter().enumerate() {
            if *state == 0 {
                continue;
            }
            let x = fog.region.min.x + (index as i32 % width);
            let y = fog.region.min.y + (index as i32 / width);
            self.explored
                .entry((x.div_euclid(CHUNK), y.div_euclid(CHUNK)))
                .or_insert_with(|| Box::new([0; 1024]))
                [(y.rem_euclid(CHUNK) * CHUNK + x.rem_euclid(CHUNK)) as usize] = 1;
        }
        self.region = Some(fog.region);
        self.live = fog.tiles.clone();
    }

    /// The region the last fog message covered.
    pub fn region(&self) -> Option<TileRect> {
        self.region
    }

    /// True when the server runs this match without fog of war.
    pub fn disabled(&self) -> bool {
        self.disabled
    }

    /// Sparse 256x256 strategic overview: `(pixel index, 1 explored | 2 visible)`.
    pub fn overview(&self) -> &[(u16, u8)] {
        &self.overview
    }

    /// Build sites need tile-exact knowledge: an overview pixel may include
    /// unseen tiles. Older exact subscriptions remain usable after moving away.
    pub(crate) fn explored_exactly(&self, tile: TileCoord) -> bool {
        if self.disabled {
            return true;
        }
        if let Some(region) = self.region.filter(|region| region.contains(tile)) {
            let index = (tile.y - region.min.y) * region.width() + tile.x - region.min.x;
            return matches!(self.live.get(index as usize), Some(1 | 2));
        }
        self.explored
            .get(&(tile.x.div_euclid(CHUNK), tile.y.div_euclid(CHUNK)))
            .is_some_and(|bits| {
                bits[(tile.y.rem_euclid(CHUNK) * CHUNK + tile.x.rem_euclid(CHUNK)) as usize] != 0
            })
    }

    pub fn visibility(&self, tile: TileCoord) -> Visibility {
        if self.disabled {
            return Visibility::Visible;
        }
        // Inside the live window the tile-exact state is authoritative.
        if let Some(region) = self.region.filter(|region| region.contains(tile)) {
            let index = (tile.y - region.min.y) * region.width() + (tile.x - region.min.x);
            return match self.live.get(index as usize) {
                Some(2) => Visibility::Visible,
                Some(1) => Visibility::Explored,
                _ => Visibility::Unexplored,
            };
        }
        let remembered = self
            .explored
            .get(&(tile.x.div_euclid(CHUNK), tile.y.div_euclid(CHUNK)))
            .is_some_and(|bits| {
                bits[(tile.y.rem_euclid(CHUNK) * CHUNK + tile.x.rem_euclid(CHUNK)) as usize] != 0
            });
        if remembered || self.overview_pixel_explored(tile) {
            Visibility::Explored
        } else {
            Visibility::Unexplored
        }
    }

    /// The overview pixel covering the middle of `tile`.
    fn overview_pixel(&self, tile: TileCoord) -> Option<usize> {
        let (width, height) = (i64::from(self.size.0), i64::from(self.size.1));
        if width <= 0 || height <= 0 || tile.x < 0 || tile.y < 0 {
            return None;
        }
        let pixel = |coordinate: i32, axis: i64| {
            (((2 * i64::from(coordinate) + 1) * OVERVIEW_AXIS) / (2 * axis)).min(OVERVIEW_AXIS - 1)
        };
        Some((pixel(tile.y, height) * OVERVIEW_AXIS + pixel(tile.x, width)) as usize)
    }

    fn overview_pixel_explored(&self, tile: TileCoord) -> bool {
        self.overview_pixel(tile)
            .is_some_and(|index| self.overview_explored[index])
    }

    #[cfg(test)]
    pub(crate) fn known_chunks_for_test(&self) -> Vec<(i32, i32)> {
        self.known_chunks()
    }

    /// Map chunks with explored or visible ground: from the live window and
    /// from the whole-map overview.
    pub(crate) fn known_chunks(&self) -> Vec<(i32, i32)> {
        let mut chunks = std::collections::BTreeSet::new();
        if let Some(region) = self.region {
            if self.disabled {
                return chunks_of(region);
            }
            let width = region.width();
            for (index, state) in self.live.iter().enumerate() {
                if *state != 0 {
                    let x = region.min.x + (index as i32 % width);
                    let y = region.min.y + (index as i32 / width);
                    chunks.insert((x.div_euclid(CHUNK), y.div_euclid(CHUNK)));
                }
            }
        }
        let (width, height) = (i64::from(self.size.0), i64::from(self.size.1));
        if width > 0 && height > 0 {
            for (index, explored) in self.overview_explored.iter().enumerate() {
                if !*explored {
                    continue;
                }
                let (px, py) = ((index % 256) as i64, (index / 256) as i64);
                let span = |pixel: i64, axis: i64| {
                    let low = pixel * axis / OVERVIEW_AXIS;
                    let high =
                        (((pixel + 1) * axis + OVERVIEW_AXIS - 1) / OVERVIEW_AXIS).max(low + 1);
                    (low as i32, (high - 1).min(axis - 1) as i32)
                };
                let ((x0, x1), (y0, y1)) = (span(px, width), span(py, height));
                for cy in y0.div_euclid(CHUNK)..=y1.div_euclid(CHUNK) {
                    for cx in x0.div_euclid(CHUNK)..=x1.div_euclid(CHUNK) {
                        chunks.insert((cx, cy));
                    }
                }
            }
        }
        chunks.into_iter().collect()
    }
}

fn chunks_of(region: TileRect) -> Vec<(i32, i32)> {
    let mut chunks = Vec::new();
    for cy in region.min.y.div_euclid(CHUNK)..=(region.max.y - 1).div_euclid(CHUNK) {
        for cx in region.min.x.div_euclid(CHUNK)..=(region.max.x - 1).div_euclid(CHUNK) {
            chunks.push((cx, cy));
        }
    }
    chunks
}
