//! Which tiles each layer wrote bodies to since this tick's movement
//! proposals, so an applied step can tell its proposal's body sweep still
//! holds without running it again (#978). The set holds this tick's writes
//! only: it is emptied when the next proposals start.
use super::{CellHasher, CollisionIndex, key};
use aoe_core::{TileCoord, WorldPosition};
use std::collections::HashSet;
use std::hash::BuildHasherDefault;

#[derive(Debug, Default)]
pub(super) struct Stamps(pub(super) HashSet<u64, BuildHasherDefault<CellHasher>>);

impl CollisionIndex {
    /// Starts a new set: every body written from here on marks its tile.
    pub(crate) fn begin_proposals(&mut self) {
        for layer in &mut self.stamps {
            layer.0.clear();
        }
        self.proposing = true;
        self.at_proposals = (self.maximum_radii, self.overfull_cells != 0);
    }

    pub(super) fn stamp(&mut self, tile: TileCoord, naval: bool) {
        if self.proposing {
            self.stamps[usize::from(naval)].0.insert(key(tile));
        }
    }

    /// Whether no body was written in the window a body sweep from `from` to
    /// `to` reads, and its global inputs are unchanged, since the proposals:
    /// a sweep that passed then passes again (#978).
    pub(crate) fn quiet_since_proposals(
        &self,
        from: WorldPosition,
        to: WorldPosition,
        own_radius: i32,
        naval: bool,
    ) -> bool {
        if !self.proposing
            || self.overfull_cells != 0
            || self.at_proposals != (self.maximum_radii, false)
        {
            return false;
        }
        let reach = i64::from(own_radius + self.maximum_radii[usize::from(naval)]);
        let tile = |v: i64| v.div_euclid(1024) as i32;
        let (x0, x1) = (
            tile(i64::from(from.x.min(to.x)) - reach),
            tile(i64::from(from.x.max(to.x)) + reach),
        );
        let (y0, y1) = (
            tile(i64::from(from.y.min(to.y)) - reach),
            tile(i64::from(from.y.max(to.y)) + reach),
        );
        let written = &self.stamps[usize::from(naval)].0;
        written.is_empty()
            || (y0..=y1).all(|y| (x0..=x1).all(|x| !written.contains(&key(TileCoord::new(x, y)))))
    }
}
