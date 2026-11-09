//! What the slot remembers of the map's fog. The server sends the state of
//! the subscribed region whole, then what changes in it; the SDK keeps older
//! regions so a bot that moves its view still knows what it explored.
use crate::model::Visibility;
use aoe_core::{TileCoord, TileRect};
use aoe_protocol::{FogDelta, FogState};
use std::collections::BTreeMap;

const CHUNK: i32 = 32;

/// The server's strategic overview is 256 x 256 pixels over the whole map.
const OVERVIEW_AXIS: i64 = 256;

#[derive(Clone, Debug)]
pub struct FogMemory {
    /// The view the server last sent whole, with its changes since laid over.
    view: Option<FogState>,
    /// Explored chunks: 0 unexplored, 1 explored (never downgraded).
    explored: BTreeMap<(i32, i32), Box<[u8; 1024]>>,
    /// Pixels of the strategic overview ever seen explored: the slot's whole
    /// map memory at coarse resolution, whatever window it subscribed to.
    overview_explored: Box<[bool; 65_536]>,
    size: (i32, i32),
}

impl Default for FogMemory {
    fn default() -> Self {
        Self {
            view: None,
            explored: BTreeMap::new(),
            overview_explored: Box::new([false; 65_536]),
            size: (0, 0),
        }
    }
}

impl FogMemory {
    pub(crate) fn apply(&mut self, fog: FogState, size: (i32, i32)) {
        self.size = size;
        self.view = Some(fog);
        self.remember(true);
    }

    /// Lays the server's changes over the held view. False when the view
    /// they were made for is not held: changes are never laid over nothing.
    pub(crate) fn apply_delta(&mut self, delta: &FogDelta) -> bool {
        let laid = self
            .view
            .as_mut()
            .is_some_and(|view| view.apply(delta).is_ok());
        if laid {
            self.remember(delta.changes_tiles());
        }
        laid
    }

    /// Keeps what the held view shows explored: its strategic pixels and,
    /// when its `tiles` changed, the tiles of its window. A view without fog
    /// has no tiles to keep.
    fn remember(&mut self, tiles: bool) {
        let Some(view) = &self.view else {
            return;
        };
        for (pixel, _) in &view.overview {
            self.overview_explored[usize::from(*pixel)] = true;
        }
        if !tiles || view.disabled {
            return;
        }
        let width = view.region.width();
        for (index, state) in view.tiles.iter().enumerate() {
            if *state == 0 {
                continue;
            }
            let x = view.region.min.x + (index as i32 % width);
            let y = view.region.min.y + (index as i32 / width);
            self.explored
                .entry((x.div_euclid(CHUNK), y.div_euclid(CHUNK)))
                .or_insert_with(|| Box::new([0; 1024]))
                [(y.rem_euclid(CHUNK) * CHUNK + x.rem_euclid(CHUNK)) as usize] = 1;
        }
    }

    /// The region the last whole fog view covered.
    pub fn region(&self) -> Option<TileRect> {
        self.view.as_ref().map(|view| view.region)
    }

    /// True when the server runs this match without fog of war.
    pub fn disabled(&self) -> bool {
        self.view.as_ref().is_some_and(|view| view.disabled)
    }

    /// Sparse 256x256 strategic overview: `(pixel index, 1 explored | 2 visible)`.
    pub fn overview(&self) -> &[(u16, u8)] {
        self.view
            .as_ref()
            .map_or(&[], |view| view.overview.as_slice())
    }

    /// The held view when its window covers `tile`, and the tile's place in it.
    fn live(&self, tile: TileCoord) -> Option<(&FogState, usize)> {
        let view = self.view.as_ref()?;
        let region = view.region.contains(tile).then_some(view.region)?;
        let index = (tile.y - region.min.y) * region.width() + tile.x - region.min.x;
        Some((view, index as usize))
    }

    /// Build sites need tile-exact knowledge: an overview pixel may include
    /// unseen tiles. Older exact subscriptions remain usable after moving away.
    pub(crate) fn explored_exactly(&self, tile: TileCoord) -> bool {
        if self.disabled() {
            return true;
        }
        if let Some((view, index)) = self.live(tile) {
            return matches!(view.tiles.get(index), Some(1 | 2));
        }
        self.explored
            .get(&(tile.x.div_euclid(CHUNK), tile.y.div_euclid(CHUNK)))
            .is_some_and(|bits| {
                bits[(tile.y.rem_euclid(CHUNK) * CHUNK + tile.x.rem_euclid(CHUNK)) as usize] != 0
            })
    }

    pub fn visibility(&self, tile: TileCoord) -> Visibility {
        if self.disabled() {
            return Visibility::Visible;
        }
        // Inside the live window the tile-exact state is authoritative.
        if let Some((view, index)) = self.live(tile) {
            return match view.tiles.get(index) {
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
        if let Some(view) = &self.view {
            let region = view.region;
            if view.disabled {
                return chunks_of(region);
            }
            let width = region.width();
            for (index, state) in view.tiles.iter().enumerate() {
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
