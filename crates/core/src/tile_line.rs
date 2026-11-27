//! Shared raster geometry for wall and road orders and their previews.
use crate::{MAX_WORLD_DIMENSION_TILES, TileCoord, TileRect};

#[derive(Clone, Copy)]
pub struct TileLine {
    from: TileCoord,
    delta: (i64, i64),
    steps: usize,
}

impl TileLine {
    pub fn new(from: TileCoord, to: TileCoord) -> Option<Self> {
        if ![from, to].into_iter().all(|tile| {
            TileRect::from_xywh(tile.x, tile.y, 1, 1)
                .valid(MAX_WORLD_DIMENSION_TILES, MAX_WORLD_DIMENSION_TILES)
        }) {
            return None;
        }
        let delta = (
            i64::from(to.x) - i64::from(from.x),
            i64::from(to.y) - i64::from(from.y),
        );
        let steps = delta.0.unsigned_abs().max(delta.1.unsigned_abs()) as usize;
        Some(Self { from, delta, steps })
    }

    pub fn count(self) -> usize {
        self.steps + 1
    }

    pub fn tiles(self) -> impl ExactSizeIterator<Item = TileCoord> {
        (0..self.count()).map(move |index| self.tile(index))
    }

    pub fn tile(self, index: usize) -> TileCoord {
        assert!(index <= self.steps, "tile index outside line");
        if self.steps == 0 {
            return self.from;
        }
        let steps = self.steps as i64;
        let offset =
            |delta: i64| ((delta * index as i64 + delta.signum() * (steps / 2)) / steps) as i32;
        TileCoord::new(
            self.from.x + offset(self.delta.0),
            self.from.y + offset(self.delta.1),
        )
    }

    /// A line advances by one tile on its dominant axis at every step.
    pub fn index_of(self, tile: TileCoord) -> Option<usize> {
        let index = if self.delta.0.abs() >= self.delta.1.abs() {
            self.from.x.abs_diff(tile.x)
        } else {
            self.from.y.abs_diff(tile.y)
        } as usize;
        (index <= self.steps && self.tile(index) == tile).then_some(index)
    }
}
