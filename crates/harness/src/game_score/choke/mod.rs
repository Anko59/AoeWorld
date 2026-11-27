//! A choke: where the ground narrows and an army has to file through. The game score stages its
//! `march` and `battle` at one real choke of the real map. This module holds what a choke is
//! (`Choke`, recorded as data beside the scenarios), how one is found in the ground (`survey`, from
//! the map's own walkable tiles) and how a run is watched at one (`watch`, a line across the neck).
mod ground;
mod report;
mod survey;
#[cfg(test)]
mod tests;
mod watch;

pub(super) use ground::MapGround;
#[cfg(test)]
pub(super) use report::survey;
pub(super) use report::survey_map;
pub(super) use survey::{
    Corridor, Grid, Ground, Saddle, across, anchors, clearance, corridor, flanks, saddles, steps,
};
pub(super) use watch::{ChokeTotals, ChokeWatch, Moves, Walker};

/// A tile of the map, `(x, y)`.
pub(super) type Tile = (i32, i32);

/// Subunits of one tile (positions are kept in them).
pub(super) const TILE: i64 = 1024;

/// A real choke of the real map, as the survey measured it and the scenarios use it. Everything is
/// data: the staging checks it against the map it is played on, so a map that changed (another hash
/// is another key anyway) or a measure that was copied wrongly fails at the start of the run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Choke {
    pub name: &'static str,
    /// The saddle the survey found: the narrowest point of the widest way between the two
    /// stretches of ground. Measuring the neck there again gives the numbers below.
    pub saddle: Tile,
    /// The tile that blocks at each end of the narrowest cross-section, west or north first: the
    /// ground between them is the choke, and the segment between their centres is the line the
    /// watch counts crossings on.
    pub ends: (Tile, Tile),
    /// Walkable tiles between the two ends.
    pub width_tiles: u32,
    /// Cross-sections in a row (both ways) no wider than the narrowest one plus two tiles.
    pub length_tiles: u32,
    /// One step along the march: `(0, 1)` for a neck that runs north to south, `(1, 0)` for west to
    /// east.
    pub flow: Tile,
    /// Where the near army stands and where the far one does: walkable ground in the open on each
    /// side, as far from the line as each other.
    pub near: Tile,
    pub far: Tile,
    /// What blocks the two sides of the neck, in words, from the survey's class of the tiles.
    pub sides: &'static str,
    /// Why the neck qualifies, in words: what it separates and what the walking field says.
    pub why: &'static str,
}

impl Choke {
    /// The tiles of the cross-section, ends excluded, in order.
    pub fn across(&self) -> Vec<Tile> {
        across(self.ends)
    }
}
