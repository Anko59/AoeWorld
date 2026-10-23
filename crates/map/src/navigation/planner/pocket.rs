//! Sealed-destination proof. A destination tile can be walkable yet sealed
//! off: a gap inside a forest, a tile boxed in by buildings. An A* search
//! from outside then explores everything it can reach before it gives up, and
//! in a 100-player match such searches took most of the world's route budget
//! (the long-game freeze). Once a leg has spent `POCKET_CHECK_WORK`, the
//! planner floods *backwards* from the destination over the same step rule
//! as the search; if that flood closes within `POCKET_TILES` tiles without
//! meeting the leg origin, no route exists and the search ends at once.
use super::search::{checked_walkable, neighbor_offset};
use super::{RouteTerrain, Terminal};
use crate::ResourceOverlay;
use aoe_core::TileCoord;
use std::collections::{BTreeSet, VecDeque};

/// Leg work after which the sealed-destination proof runs (once per planner).
pub(super) const POCKET_CHECK_WORK: u32 = 1_024;
/// Largest sealed region the proof recognises; a larger one is inconclusive.
const POCKET_TILES: usize = 1_024;

/// True when `from` may step to its neighbour at `offset`, by exactly the
/// rule the search expands with.
fn step_allowed(
    terrain: &dyn RouteTerrain,
    overlay: &ResourceOverlay,
    from: TileCoord,
    offset: (i32, i32),
    cancelled: &dyn Fn() -> bool,
) -> Result<bool, Terminal> {
    let (dx, dy) = offset;
    let next = TileCoord::new(from.x + dx, from.y + dy);
    if dx != 0 && dy != 0 {
        let horizontal = TileCoord::new(from.x + dx, from.y);
        let vertical = TileCoord::new(from.x, from.y + dy);
        if !(checked_walkable(terrain, overlay, horizontal, cancelled)?
            && checked_walkable(terrain, overlay, vertical, cancelled)?
            && terrain.route_edge_clear(from, horizontal, cancelled)?
            && terrain.route_edge_clear(from, vertical, cancelled)?)
        {
            return Ok(false);
        }
    }
    Ok(checked_walkable(terrain, overlay, next, cancelled)?
        && terrain.route_edge_clear(from, next, cancelled)?)
}

/// Retained reverse frontier. Each `step` probes at most one neighbour, just
/// like an A* expansion, so even a one-unit poll can suspend this proof.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PocketSearch {
    pub(super) seen: BTreeSet<TileCoord>,
    pub(super) queue: VecDeque<TileCoord>,
    pub(super) direction: u8,
}

impl PocketSearch {
    pub(super) fn new(destination: TileCoord) -> Self {
        Self {
            seen: BTreeSet::from([destination]),
            queue: VecDeque::from([destination]),
            direction: 0,
        }
    }

    /// None means pending; Some(false) is inconclusive; Some(true) proves sealed.
    pub(super) fn step(
        &mut self,
        terrain: &dyn RouteTerrain,
        overlay: &ResourceOverlay,
        origin: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<bool>, Terminal> {
        if cancelled() {
            return Err(crate::EnvironmentPageError::Cancelled.into());
        }
        let Some(&tile) = self.queue.front() else {
            return Ok(Some(true));
        };
        let (dx, dy) = neighbor_offset(self.direction);
        self.direction += 1;
        if self.direction == 8 {
            self.queue.pop_front();
            self.direction = 0;
        }
        let previous = TileCoord::new(tile.x - dx, tile.y - dy);
        if !self.seen.contains(&previous)
            && checked_walkable(terrain, overlay, previous, cancelled)?
            && step_allowed(terrain, overlay, previous, (dx, dy), cancelled)?
        {
            if previous == origin || self.seen.len() >= POCKET_TILES {
                return Ok(Some(false));
            }
            self.seen.insert(previous);
            self.queue.push_back(previous);
        }
        Ok(self.queue.is_empty().then_some(true))
    }

    pub(super) fn retained_entries(&self) -> usize {
        self.seen.len() + self.queue.len()
    }

    pub(super) fn update_hash(&self, hash: &mut blake3::Hasher) {
        hash.update(&[self.direction]);
        for tiles in [
            self.seen.iter().copied().collect::<Vec<_>>(),
            self.queue.iter().copied().collect(),
        ] {
            hash.update(&(tiles.len() as u64).to_le_bytes());
            for tile in tiles {
                hash.update(&tile.x.to_le_bytes());
                hash.update(&tile.y.to_le_bytes());
            }
        }
    }
}
